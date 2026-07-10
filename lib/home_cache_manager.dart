import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';
import 'intelligence_engine.dart';

class HomeCacheManager {
  static File _getCacheFile() {
    final home = Platform.environment['HOME'];
    if (home != null) {
      final dir = Directory('$home/.config/meduza');
      if (!dir.existsSync()) {
        dir.createSync(recursive: true);
      }
      return File('${dir.path}/home_cache.json');
    }
    return File('home_cache.json');
  }

  static Future<Map<String, List<TrackItem>>> loadCache() async {
    try {
      final file = _getCacheFile();
      if (await file.exists()) {
        final content = await file.readAsString();
        final Map<String, dynamic> jsonMap = jsonDecode(content);
        final Map<String, List<TrackItem>> result = {};
        
        jsonMap.forEach((query, data) {
          final timestamp = data['timestamp'] as int;
          final tracksJson = data['tracks'] as List;
          // Cache expires after 24 hours to keep recommendations fresh
          if (DateTime.now().millisecondsSinceEpoch - timestamp < 24 * 60 * 60 * 1000) {
            result[query] = tracksJson.map((t) {
              return TrackItem(
                title: t['title'] ?? 'Unknown Title',
                artist: t['artist'] ?? 'Unknown Artist',
                mediaId: t['mediaId'] ?? '',
                thumbnailUrl: t['thumbnailUrl'] ?? '',
                duration: t['durationMs'] != null ? Duration(milliseconds: t['durationMs']) : null,
              );
            }).toList();
          }
        });
        return result;
      }
    } catch (e) {
      debugPrint('[HomeCacheManager] Load cache error: $e');
    }
    return {};
  }

  static Future<void> saveCache(Map<String, List<TrackItem>> cache) async {
    try {
      final file = _getCacheFile();
      final Map<String, dynamic> jsonMap = {};
      cache.forEach((query, tracks) {
        jsonMap[query] = {
          'timestamp': DateTime.now().millisecondsSinceEpoch,
          'tracks': tracks.map((t) => {
            'title': t.title,
            'artist': t.artist,
            'mediaId': t.mediaId,
            'thumbnailUrl': t.thumbnailUrl,
            'durationMs': t.duration?.inMilliseconds,
          }).toList(),
        };
      });
      await file.writeAsString(jsonEncode(jsonMap));
    } catch (e) {
      debugPrint('[HomeCacheManager] Save cache error: $e');
    }
  }
}
