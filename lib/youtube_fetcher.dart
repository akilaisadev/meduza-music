import 'package:youtube_explode_dart/youtube_explode_dart.dart';
import 'dart:convert';
import 'dart:io';

/// A YouTube fetcher with an in-memory LRU stream URL cache
/// to dramatically speed up repeated playback requests.
class YouTubeFetcher {
  static final YoutubeExplode _yt = YoutubeExplode();

  // LRU cache: insertion-ordered map, evict first entry when full
  static final _streamCache = <String, String>{};
  static const _cacheMaxSize = 25;

  /// Search for tracks and return a list of results
  Future<List<Video>> searchTracks(String query) async {
    try {
      final searchList = await _yt.search.search(query);
      if (searchList.isNotEmpty) {
        return searchList.take(20).toList();
      }
    } catch (e) {
      debugLog('Library search failed, attempting custom fallback scraper: $e');
    }
    return _customSearchScrape(query);
  }

  Future<List<Video>> _customSearchScrape(String query) async {
    final client = HttpClient();
    try {
      final uri = Uri.parse('https://www.youtube.com/youtubei/v1/search');
      final request = await client.postUrl(uri);
      request.headers.set('Content-Type', 'application/json');
      request.headers.set('User-Agent', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36');
      
      final body = {
        'query': query,
        'context': {
          'client': {
            'clientName': 'WEB',
            'clientVersion': '2.20240501.01.00',
            'hl': 'en',
            'gl': 'US'
          }
        }
      };
      
      request.write(jsonEncode(body));
      final response = await request.close();
      if (response.statusCode != 200) {
        return [];
      }
      
      final resBody = await response.transform(utf8.decoder).join();
      final data = jsonDecode(resBody);
      
      final contents = data['contents']?['twoColumnSearchResultsRenderer']?['primaryContents']?['sectionListRenderer']?['contents'];
      if (contents == null || contents is! List) {
        return [];
      }
      
      List<dynamic>? items;
      for (var section in contents) {
        if (section['itemSectionRenderer'] != null) {
          items = section['itemSectionRenderer']['contents'];
          break;
        }
      }
      
      if (items == null) {
        return [];
      }
      
      final List<Video> videos = [];
      for (var item in items) {
        if (item['videoRenderer'] != null) {
          final videoMap = item['videoRenderer'];
          final videoIdStr = videoMap['videoId'] as String?;
          if (videoIdStr == null) continue;
          
          final title = videoMap['title']?['runs']?[0]?['text'] as String? ?? 'Unknown';
          final author = videoMap['ownerText']?['runs']?[0]?['text'] as String? ?? 'Unknown';
          final durationStr = videoMap['lengthText']?['simpleText'] as String? ?? '';
          
          Duration? duration;
          if (durationStr.isNotEmpty) {
            final parts = durationStr.split(':');
            if (parts.length == 2) {
              duration = Duration(minutes: int.tryParse(parts[0]) ?? 0, seconds: int.tryParse(parts[1]) ?? 0);
            } else if (parts.length == 3) {
              duration = Duration(hours: int.tryParse(parts[0]) ?? 0, minutes: int.tryParse(parts[1]) ?? 0, seconds: int.tryParse(parts[2]) ?? 0);
            }
          }
          
          final videoId = VideoId(videoIdStr);
          // Use a safe dummy channel ID — ChannelId() throws on free-text author names
          final channelId = ChannelId('UC${videoIdStr.padRight(22, '0')}');
          final thumbnails = ThumbnailSet(videoIdStr);
          const engagement = Engagement(0, 0, 0);
          
          final video = Video(
            videoId,
            title,
            author,
            channelId,
            DateTime.now(),
            'today',
            DateTime.now(),
            '',
            duration,
            thumbnails,
            [],
            engagement,
            false,
          );
          
          videos.add(video);
          if (videos.length >= 20) {
            break;
          }
        }
      }
      return videos;
    } catch (e) {
      debugLog('Error in InnerTube search search fallback: $e');
    } finally {
      client.close();
    }
    return [];
  }

  /// Get a Playlist and all its tracks (Using Search as a robust fallback)
  Future<Map<String, dynamic>?> getPlaylistDetails(String query) async {
    try {
      final videos = await searchTracks(query);
      if (videos.isEmpty) return null;
      
      return {
        'title': query,
        'author': 'YouTube Mix',
        'thumbnail': videos.first.thumbnails.highResUrl,
        'videos': videos,
      };
    } catch (e) {
      debugLog('Error fetching playlist via search: $e');
    }
    return null;
  }

  /// Get related tracks by searching for "<artist> <title> mix"
  Future<List<Video>> getRelatedTracks(String videoId, {String? title, String? artist}) async {
    try {
      String query;
      if (title != null && artist != null) {
        query = '$artist $title mix';
      } else {
        // Fetch the video info first to build a related query
        final video = await _yt.videos.get(videoId);
        query = '${video.author} ${video.title} mix';
      }
      final results = await searchTracks(query);
      // Skip the first result (likely the same video) and take the next batch
      return results
          .where((v) => v.id.value != videoId)
          .take(15)
          .toList();
    } catch (e) {
      debugLog('Error fetching related tracks: $e');
    }
    return [];
  }

  Future<String?> getAudioStreamUrl(String videoId) async {
    // Return from cache if available
    if (_streamCache.containsKey(videoId)) {
      debugLog('Cache hit for $videoId');
      // Move to end (LRU)
      final url = _streamCache.remove(videoId)!;
      _streamCache[videoId] = url;
      return url;
    }

    // Try high-speed direct InnerTube resolution first (takes ~150ms instead of 3-4s)
    final fastUrl = await _resolveStreamInnerTube(videoId);
    if (fastUrl != null) {
      debugLog('InnerTube fast stream resolved: $fastUrl');
      if (_streamCache.length >= _cacheMaxSize) {
        _streamCache.remove(_streamCache.keys.first);
      }
      _streamCache[videoId] = fastUrl;
      return fastUrl;
    }

    debugLog('Fast stream failed, falling back to slow YoutubeExplode client...');
    try {
      final manifest = await _yt.videos.streamsClient.getManifest(videoId);
      
      String? bestUrl;

      // Priority 1: Muxed progressive streams (extremely reliable, progressive HTTP, non-DASH)
      final muxed = manifest.muxed.sortByVideoQuality().toList();
      if (muxed.isNotEmpty) {
        bestUrl = muxed.first.url.toString();
        debugLog('Found muxed progressive stream: $bestUrl');
      }

      // Priority 2: High-bitrate audio-only m4a (AAC) stream as fallback
      if (bestUrl == null) {
        final m4aStreams = manifest.audioOnly
            .where((s) => s.container.name == 'mp4')
            .sortByBitrate()
            .toList();
        if (m4aStreams.isNotEmpty) {
          bestUrl = m4aStreams.last.url.toString();
          debugLog('Fallback to m4a audio stream: $bestUrl');
        }
      }
      
      // Priority 3: Any audio-only stream (webm/opus)
      if (bestUrl == null) {
        final anyAudio = manifest.audioOnly.sortByBitrate().toList();
        if (anyAudio.isNotEmpty) {
          bestUrl = anyAudio.last.url.toString();
          debugLog('Fallback to general audio stream: $bestUrl');
        }
      }

      if (bestUrl != null) {
        // Evict oldest if cache is full
        if (_streamCache.length >= _cacheMaxSize) {
          _streamCache.remove(_streamCache.keys.first);
        }
        _streamCache[videoId] = bestUrl;
      }

      return bestUrl;
    } catch (e) {
      debugLog('Error getting audio stream fallback: $e');
    }
    return null;
  }

  Future<String?> _resolveStreamInnerTube(String videoId) async {
    // Fire both clients in parallel - whichever resolves first wins
    final results = await Future.wait([
      _resolveStreamInnerTubeWithClient(
        videoId,
        clientName: 'ANDROID_TESTSUITE',
        clientVersion: '1.9.3',
        userAgent: 'com.google.android.youtube/19.16.34 (Linux; U; Android 11) gzip',
      ),
      _resolveStreamInnerTubeWithClient(
        videoId,
        clientName: 'ANDROID_VR',
        clientVersion: '1.37',
        userAgent: 'com.google.android.apps.youtube.vr.oculus/1.37 (Linux; U; Android 12; en_US; Quest 3; Build/SQ3A.220605.009.A1; Cronet/107.0.5284.2)',
      ),
    ]).timeout(const Duration(seconds: 4), onTimeout: () => [null, null]);

    // Return first non-null result
    return results.firstWhere((url) => url != null, orElse: () => null);
  }

  Future<String?> _resolveStreamInnerTubeWithClient(
    String videoId, {
    required String clientName,
    required String clientVersion,
    required String userAgent,
  }) async {
    final client = HttpClient()
      ..connectionTimeout = const Duration(seconds: 3);
    try {
      final uri = Uri.parse('https://www.youtube.com/youtubei/v1/player');
      final request = await client.postUrl(uri);
      request.headers.set('Content-Type', 'application/json');
      request.headers.set('User-Agent', userAgent);
      
      final body = {
        'videoId': videoId,
        'context': {
          'client': {
            'clientName': clientName,
            'clientVersion': clientVersion,
            'hl': 'en',
            'gl': 'US',
            'utcOffsetMinutes': 0
          }
        }
      };
      
      request.write(jsonEncode(body));
      final response = await request.close();
      if (response.statusCode != 200) {
        return null;
      }
      
      final resBody = await response.transform(utf8.decoder).join();
      final data = jsonDecode(resBody);
      
      final status = data['playabilityStatus']?['status'];
      if (status != 'OK') {
        return null;
      }
      
      final adaptiveFormats = data['streamingData']?['adaptiveFormats'];
      if (adaptiveFormats is List) {
        String? bestUrl;
        int maxBitrate = 0;
        
        for (var format in adaptiveFormats) {
          final mimeType = format['mimeType'] as String? ?? '';
          if (mimeType.contains('audio/')) {
            final url = format['url'] as String? ?? '';
            final bitrate = format['bitrate'] as int? ?? 0;
            if (url.isNotEmpty && bitrate > maxBitrate) {
              bestUrl = url;
              maxBitrate = bitrate;
            }
          }
        }
        return bestUrl;
      }
    } catch (e) {
      debugLog('Error in InnerTube stream resolution ($clientName): $e');
    } finally {
      client.close();
    }
    return null;
  }

  void dispose() {
    // Don't close the shared static client
  }

  void debugLog(String msg) {
    // ignore: avoid_print
    print('[YouTubeFetcher] $msg');
  }
}
