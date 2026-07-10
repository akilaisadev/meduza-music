import 'dart:math';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';

/// MEDUZA Flagship User Intelligence System
///
/// A context-aware, multi-signal, learning music engine that reads user taste
/// dynamically through implicit/explicit feedback, adapts preferences, 
/// penalizes skips, and persists learned data on disk.
class MeduzaIntelligenceEngine {
  static final _random = Random();

  // --- Taste Profile Databases (Persisted) ---
  static final Map<String, int> artistPlayCounts = {};
  static final Map<String, int> trackPlayCounts = {};
  static final Map<String, int> trackSkips = {};
  static final Map<String, double> genreAffinity = {};
  static final Set<String> recentlyPlayedIds = {};
  static final Set<String> likedTrackIds = {};

  static File _getProfileFile() {
    final home = Platform.environment['HOME'];
    if (home != null) {
      final dir = Directory('$home/.config/meduza');
      if (!dir.existsSync()) {
        dir.createSync(recursive: true);
      }
      return File('${dir.path}/taste_profile.json');
    }
    return File('taste_profile.json');
  }

  /// Loads the persisted user taste profile from disk
  static Future<void> loadProfile() async {
    try {
      final file = _getProfileFile();
      if (await file.exists()) {
        final content = await file.readAsString();
        final jsonMap = jsonDecode(content) as Map<String, dynamic>;

        artistPlayCounts.clear();
        for (final entry in (jsonMap['artistPlayCounts'] as Map? ?? {}).entries) {
          artistPlayCounts[entry.key.toString()] = entry.value as int;
        }

        trackPlayCounts.clear();
        for (final entry in (jsonMap['trackPlayCounts'] as Map? ?? {}).entries) {
          trackPlayCounts[entry.key.toString()] = entry.value as int;
        }

        trackSkips.clear();
        for (final entry in (jsonMap['trackSkips'] as Map? ?? {}).entries) {
          trackSkips[entry.key.toString()] = entry.value as int;
        }

        genreAffinity.clear();
        for (final entry in (jsonMap['genreAffinity'] as Map? ?? {}).entries) {
          genreAffinity[entry.key.toString()] = (entry.value as num).toDouble();
        }

        recentlyPlayedIds.clear();
        for (final v in (jsonMap['recentlyPlayedIds'] as List? ?? [])) {
          recentlyPlayedIds.add(v.toString());
        }

        debugPrint('[IntelligenceEngine] Persisted user taste database loaded successfully.');
      }
    } catch (e) {
      debugPrint('[IntelligenceEngine] Error loading taste database: $e');
    }
  }

  /// Saves the active user taste profile to disk
  static Future<void> saveProfile() async {
    try {
      final file = _getProfileFile();
      final content = jsonEncode({
        'artistPlayCounts': artistPlayCounts,
        'trackPlayCounts': trackPlayCounts,
        'trackSkips': trackSkips,
        'genreAffinity': genreAffinity,
        'recentlyPlayedIds': recentlyPlayedIds.toList(),
      });
      await file.writeAsString(content);
    } catch (e) {
      debugPrint('[IntelligenceEngine] Error saving taste database: $e');
    }
  }

  /// Implicit Feedback: Records a successful track listen and increases affinity
  static void recordPlay(TrackItem track) {
    final artistKey = track.artist.toLowerCase().trim();
    final titleKey = track.title.toLowerCase().trim();
    final mediaId = track.mediaId;

    artistPlayCounts[artistKey] = (artistPlayCounts[artistKey] ?? 0) + 1;
    trackPlayCounts[mediaId] = (trackPlayCounts[mediaId] ?? 0) + 1;

    recentlyPlayedIds.add(mediaId);
    if (recentlyPlayedIds.length > 50) {
      recentlyPlayedIds.remove(recentlyPlayedIds.first);
    }

    // Adapt genre and style keyword affinities positively
    final keywords = _extractKeywords('$titleKey $artistKey');
    for (final kw in keywords) {
      genreAffinity[kw] = (genreAffinity[kw] ?? 0.0) + 0.15;
    }

    saveProfile();
  }

  /// Implicit Feedback: Records an early skip and decays affinity/genre weights
  static void recordSkip(TrackItem track) {
    final artistKey = track.artist.toLowerCase().trim();
    final titleKey = track.title.toLowerCase().trim();
    final mediaId = track.mediaId;

    trackSkips[mediaId] = (trackSkips[mediaId] ?? 0) + 1;

    // Adapt genre and style keyword affinities negatively
    final keywords = _extractKeywords('$titleKey $artistKey');
    for (final kw in keywords) {
      final val = genreAffinity[kw] ?? 0.0;
      genreAffinity[kw] = max(val - 0.12, -1.0); // allow negative weights for disliked aesthetics
    }

    // Decay general artist play count to adapt recommendations away from skipped creators
    final plays = artistPlayCounts[artistKey] ?? 0;
    if (plays > 0) {
      artistPlayCounts[artistKey] = max(plays - 1, 0);
    }

    saveProfile();
  }

  /// Keyword Extractor: Separates and cleans search metadata, stripping stopwords
  static List<String> _extractKeywords(String text) {
    final words = text.split(RegExp(r'\s+'));
    final filtered = <String>[];
    const stopwords = {
      'the', 'a', 'an', 'and', 'or', 'but', 'in', 'on', 'at', 'to', 'for', 'with', 'by',
      'of', 'is', 'are', 'was', 'were', 'it', 'you', 'me', 'him', 'her', 'them', 'us',
      'my', 'your', 'his', 'its', 'their', 'our', 'this', 'that', 'these', 'those', 'lyrics',
      'audio', 'official', 'video', 'music', 'full', 'hd', 'hq', 'remix', 'feat', 'ft'
    };
    for (final w in words) {
      final clean = w.replaceAll(RegExp(r'[^a-zA-Z0-9]'), '').toLowerCase();
      if (clean.length > 2 && !stopwords.contains(clean)) {
        filtered.add(clean);
      }
    }
    return filtered;
  }

  // --- Dynamic Track Scoring Algorithm ---
  static double scoreTrack({
    required String title,
    required String artist,
    required String mediaId,
    int? hourOfDay,
  }) {
    double score = 1.0;

    final artistKey = artist.toLowerCase().trim();
    final titleKey = title.toLowerCase().trim();

    // Signal 1: Taste Affinity (Artist plays)
    final artistPlays = artistPlayCounts[artistKey] ?? 0;
    int maxCount = 1;
    if (artistPlayCounts.isNotEmpty) {
      maxCount = artistPlayCounts.values.reduce(max);
      if (maxCount < 1) maxCount = 1;
    }
    final affinityScore = sqrt(artistPlays / maxCount);
    score += affinityScore * 2.0;

    // Signal 2: Learned Keyword/Genre Affinity (Learns styles like study, lofi, rock, workout)
    final keywords = _extractKeywords('$titleKey $artistKey');
    double genreScore = 0.0;
    for (final kw in keywords) {
      genreScore += genreAffinity[kw] ?? 0.0;
    }
    score += genreScore * 1.5;

    // Signal 3: Skip-to-Play Ratio Penalty
    final plays = trackPlayCounts[mediaId] ?? 0;
    final skips = trackSkips[mediaId] ?? 0;
    if (skips > 0) {
      final ratio = skips / (plays + skips);
      score *= (1.0 - ratio * 0.85); // High skip rates aggressively suppress suggestions
    }

    // Signal 3b: Explicit Favorites Boost
    if (likedTrackIds.contains(mediaId)) {
      score += 6.5; // Major boost to user-liked items
    }

    // Signal 4: Recency Penalty (Prevents repeating recently played tracks)
    if (recentlyPlayedIds.contains(mediaId)) {
      score *= 0.15;
    }

    // Signal 5: Time of Day Energy Arc Boost
    final preferredTags = getEnergyArcTags(hourOfDay);
    final trackTags = detectMoodTags(title, artist);
    final overlap = preferredTags.intersection(trackTags).length;
    score += overlap * 0.4;

    // Signal 6: Mild randomness seed to keep experience fresh
    score += _random.nextDouble() * 0.35;

    return max(score, 0.001);
  }

  // --- Intelligent Shuffle / Suggestion Ranker ---
  static List<int> shuffleWithIntelligence({
    required List<TrackItem> items,
    int? hourOfDay,
  }) {
    if (items.length <= 1) return List.generate(items.length, (i) => i);

    final remaining = List.generate(items.length, (i) => i);
    final result = <int>[];
    final recentArtists = <String>[];

    while (remaining.isNotEmpty) {
      final scores = remaining.map((idx) {
        final item = items[idx];
        double finalScore = scoreTrack(
          title: item.title,
          artist: item.artist,
          mediaId: item.mediaId,
          hourOfDay: hourOfDay,
        );

        // Diversity window penalty: avoid clustered suggestions from the same artist consecutively
        final artistKey = item.artist.toLowerCase().trim();
        final appearances = recentArtists.where((a) => a == artistKey).length;
        if (appearances > 0) {
          finalScore *= exp(-appearances * 1.2);
        }

        return finalScore;
      }).toList();

      final totalWeight = scores.fold<double>(0, (p, c) => p + c);
      double pick = _random.nextDouble() * totalWeight;
      int chosenPos = 0;

      for (int i = 0; i < scores.length; i++) {
        pick -= scores[i];
        if (pick <= 0) {
          chosenPos = i;
          break;
        }
      }

      final chosenOrigIdx = remaining.removeAt(chosenPos);
      result.add(chosenOrigIdx);

      final chosenArtist = items[chosenOrigIdx].artist.toLowerCase().trim();
      if (chosenArtist.isNotEmpty) {
        recentArtists.add(chosenArtist);
        if (recentArtists.length > 5) recentArtists.removeAt(0);
      }
    }

    return result;
  }

  // -- Mood Tags & Energy Arcs --
  static Set<MoodTag> detectMoodTags(String title, String artist) {
    final combined = '$title $artist'.toLowerCase();
    final tags = <MoodTag>{};

    const upbeatKeywords = ['dance', 'party', 'club', 'hit', 'pop', 'feel good', 'summer', 'happy', 'fun', 'bop'];
    const chillKeywords = ['chill', 'lofi', 'lo-fi', 'relax', 'slow', 'calm', 'easy', 'coffee', 'night drive', 'bedroom'];
    const epicKeywords = ['epic', 'power', 'anthem', 'rise', 'fire', 'hype', 'battle', 'boss', 'strong', 'warrior'];
    const ambientKeywords = ['ambient', 'space', 'dream', 'sleep', 'meditation', 'wave', 'ocean', 'forest', 'rain', 'ethereal'];
    const danceKeywords = ['edm', 'techno', 'electro', 'house', 'bass', 'drop', 'remix', 'rave', 'synthwave', 'disco'];
    const romanticKeywords = ['love', 'heart', 'kiss', 'romance', 'soul', 'tender', 'forever', 'darling', 'sweetheart', 'adore'];
    const melancholyKeywords = ['sad', 'broken', 'cry', 'alone', 'tears', 'miss', 'lost', 'goodbye', 'hurt', 'empty'];
    const focusKeywords = ['study', 'focus', 'work', 'productivity', 'concentrate', 'instrumental', 'piano', 'classical', 'jazz'];
    const partyKeywords = ['turn up', 'lit', 'shot', 'drunk', 'weekend', 'friday', 'saturday', 'crowd', 'loud', 'anthem'];

    if (upbeatKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.upbeat);
    if (chillKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.chill);
    if (epicKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.epic);
    if (ambientKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.ambient);
    if (danceKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.dance);
    if (romanticKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.romantic);
    if (melancholyKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.melancholy);
    if (focusKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.focus);
    if (partyKeywords.any((k) => combined.contains(k))) tags.add(MoodTag.party);

    if (tags.isEmpty) tags.add(MoodTag.unknown);
    return tags;
  }

  static Set<MoodTag> getEnergyArcTags([int? hourOfDay]) {
    final hour = hourOfDay ?? DateTime.now().hour;
    if (hour >= 5 && hour <= 8) return {MoodTag.upbeat, MoodTag.focus, MoodTag.chill};
    if (hour >= 9 && hour <= 11) return {MoodTag.focus, MoodTag.epic, MoodTag.upbeat};
    if (hour >= 12 && hour <= 13) return {MoodTag.upbeat, MoodTag.dance, MoodTag.party};
    if (hour >= 14 && hour <= 17) return {MoodTag.upbeat, MoodTag.dance, MoodTag.romantic};
    if (hour >= 18 && hour <= 20) return {MoodTag.chill, MoodTag.romantic, MoodTag.melancholy};
    if (hour >= 21 && hour <= 23) return {MoodTag.ambient, MoodTag.chill, MoodTag.melancholy};
    return {MoodTag.ambient, MoodTag.focus, MoodTag.chill}; // 00:00 - 04:59
  }
}

enum MoodTag {
  upbeat,
  chill,
  epic,
  ambient,
  dance,
  romantic,
  melancholy,
  focus,
  party,
  unknown,
}

class TrackItem {
  final String title;
  final String artist;
  final String mediaId;
  final String thumbnailUrl;
  final Duration? duration;

  TrackItem({
    required this.title,
    required this.artist,
    required this.mediaId,
    this.thumbnailUrl = '',
    this.duration,
  });
}
