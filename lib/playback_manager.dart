import 'package:flutter/foundation.dart';
import 'package:media_kit/media_kit.dart';
import 'dart:async';
import 'intelligence_engine.dart';
import 'youtube_fetcher.dart';

class PlaybackManager extends ChangeNotifier {
  final Player _player = Player(
    configuration: const PlayerConfiguration(
      logLevel: MPVLogLevel.warn,
    ),
  );

  List<TrackItem> _queue = [];
  List<TrackItem> _originalQueue = [];
  int _currentIndex = -1;

  Duration _position = Duration.zero;
  Duration _duration = Duration.zero;
  bool _isPlaying = false;
  bool _isBuffering = false;
  bool _isRadioActive = false;
  bool _isFetchingRelated = false;
  bool _isLooping = false;
  bool _isShuffling = false;
  String? _error;

  // Cancellation: if this changes before stream URL resolves, skip opening
  int _playGeneration = 0;

  StreamSubscription<Duration>? _positionSubscription;
  StreamSubscription<Duration>? _durationSubscription;
  StreamSubscription<bool>? _playingSubscription;
  StreamSubscription<bool>? _bufferingSubscription;
  StreamSubscription<bool>? _completedSubscription;
  StreamSubscription<String>? _errorSubscription;
  StreamSubscription<double>? _volumeSubscription;

  double _volume = 100.0;

  final YouTubeFetcher _youtubeFetcher = YouTubeFetcher();

  // Intelligence Engine Data
  final Map<String, int> artistPlayCounts = {};
  final Set<String> recentlyPlayedIds = {};
  final List<TrackItem> recentlyPlayed = [];
  double? currentHue;

  TrackItem? _activePlayingTrack;
  DateTime? _activeTrackStartTime;

  PlaybackManager() {
    // Load persisted user taste profile
    MeduzaIntelligenceEngine.loadProfile().then((_) {
      _syncTasteData();
      notifyListeners();
    });

    _positionSubscription = _player.stream.position.listen((pos) {
      _position = pos;
      notifyListeners();
    });

    _durationSubscription = _player.stream.duration.listen((dur) {
      _duration = dur;
      notifyListeners();
    });

    _playingSubscription = _player.stream.playing.listen((playing) {
      _isPlaying = playing;
      notifyListeners();
    });

    _bufferingSubscription = _player.stream.buffering.listen((buffering) {
      _isBuffering = buffering;
      notifyListeners();
    });

    _completedSubscription = _player.stream.completed.listen((completed) async {
      if (completed) {
        if (_isLooping) {
          await _player.seek(Duration.zero);
          await _player.play();
        } else {
          next();
        }
      }
    });

    _errorSubscription = _player.stream.error.listen((error) {
      debugPrint('[PlaybackManager] Player error: $error');
      _error = error;
      _isBuffering = false;
      _isPlaying = false;
      notifyListeners();
    });

    _volumeSubscription = _player.stream.volume.listen((v) {
      _volume = v;
      notifyListeners();
    });
  }

  void _syncTasteData() {
    artistPlayCounts.clear();
    artistPlayCounts.addAll(MeduzaIntelligenceEngine.artistPlayCounts);
    recentlyPlayedIds.clear();
    recentlyPlayedIds.addAll(MeduzaIntelligenceEngine.recentlyPlayedIds);
  }

  void _recordFeedbackForActiveTrack() {
    final track = _activePlayingTrack;
    final startTime = _activeTrackStartTime;
    if (track == null || startTime == null) return;

    final elapsedSeconds = DateTime.now().difference(startTime).inSeconds;
    if (elapsedSeconds < 20) {
      debugPrint('[PlaybackManager] Track skipped early (${elapsedSeconds}s): ${track.title}');
      MeduzaIntelligenceEngine.recordSkip(track);
    } else {
      debugPrint('[PlaybackManager] Track listened fully/partially (${elapsedSeconds}s): ${track.title}');
      MeduzaIntelligenceEngine.recordPlay(track);
    }

    _syncTasteData();
    _activePlayingTrack = null;
    _activeTrackStartTime = null;
  }

  void updateLikedTrackIds(Set<String> ids) {
    MeduzaIntelligenceEngine.likedTrackIds.clear();
    MeduzaIntelligenceEngine.likedTrackIds.addAll(ids);
  }

  // --- State Getters ---

  bool get isPlaying => _isPlaying;
  bool get isBuffering => _isBuffering;
  bool get isRadioActive => _isRadioActive;
  bool get isLooping => _isLooping;
  bool get isShuffling => _isShuffling;
  Duration get position => _position;
  Duration get duration => _duration;
  double get volume => _volume;

  void setVolume(double val) {
    _volume = val.clamp(0.0, 100.0);
    _player.setVolume(_volume);
    notifyListeners();
  }
  String? get error => _error;
  int get currentIndex => _currentIndex;

  TrackItem? get currentTrack {
    if (_currentIndex >= 0 && _currentIndex < _queue.length) {
      return _queue[_currentIndex];
    }
    return null;
  }

  List<TrackItem> get queue => _queue;

  bool get hasNext => _currentIndex < _queue.length - 1 || _isRadioActive;
  bool get hasPrevious => _currentIndex > 0;

  void _resetTransportState({bool clearError = false}) {
    _position = Duration.zero;
    _duration = Duration.zero;
    _isPlaying = false;
    _isBuffering = false;
    if (clearError) {
      _error = null;
    }
  }

  // --- Controls ---

  Future<void> playPause() async {
    if (currentTrack == null) return;
    if (_isPlaying) {
      await _player.pause();
    } else {
      await _player.play();
    }
  }

  Future<void> seek(Duration position) async {
    await _player.seek(position);
  }

  void toggleLoop() {
    _isLooping = !_isLooping;
    _player.setPlaylistMode(_isLooping ? PlaylistMode.single : PlaylistMode.none);
    notifyListeners();
  }

  void toggleShuffle() {
    _isShuffling = !_isShuffling;
    if (_isShuffling) {
      intelligentShuffle();
    } else {
      // Restore original order starting after current track
      if (_queue.isNotEmpty && _currentIndex >= 0 && _currentIndex < _queue.length) {
        final currentTrackItem = _queue[_currentIndex];
        final origIdx = _originalQueue.indexWhere((t) => t.mediaId == currentTrackItem.mediaId);
        if (origIdx != -1) {
          final upcomingOriginal = _originalQueue.sublist(origIdx + 1);
          _queue = [
            ..._queue.sublist(0, _currentIndex + 1),
            ...upcomingOriginal,
          ];
        }
      }
    }
    notifyListeners();
  }

  Future<void> next() async {
    if (_queue.isEmpty) return;
    if (_currentIndex < _queue.length - 1) {
      _currentIndex++;
      await _playCurrent();
      _checkAutoFetch();
    } else if (_isRadioActive) {
      // Radio mode: fetch more tracks
      await _checkAutoFetch();
      if (_currentIndex < _queue.length - 1) {
        _currentIndex++;
        await _playCurrent();
      }
    } else {
      _isPlaying = false;
      notifyListeners();
    }
  }

  Future<void> previous() async {
    if (_position.inSeconds > 3) {
      await _player.seek(Duration.zero);
      return;
    }
    if (_currentIndex > 0) {
      _currentIndex--;
      await _playCurrent();
    }
  }

  // --- Queue Management ---

  void setQueue(List<TrackItem> newQueue, {int initialIndex = 0}) {
    _originalQueue = List.from(newQueue);
    _error = null;

    if (newQueue.isEmpty) {
      _queue = [];
      _currentIndex = -1;
      _playGeneration++;
      _player.stop();
      _resetTransportState();
      notifyListeners();
      return;
    }

    if (_isShuffling) {
      final first = newQueue[initialIndex];
      final rest = List<TrackItem>.from(newQueue)..removeAt(initialIndex);
      final shuffledIndices = MeduzaIntelligenceEngine.shuffleWithIntelligence(
        items: rest,
      );
      final shuffledRest = shuffledIndices.map((i) => rest[i]).toList();
      _queue = [first, ...shuffledRest];
      _currentIndex = 0;
    } else {
      _queue = List.from(newQueue);
      _currentIndex = initialIndex.clamp(0, _queue.length - 1);
    }

    notifyListeners();
    if (_queue.isNotEmpty) {
      _playCurrent();
    }
    _checkAutoFetch();
  }

  Future<void> playNow(TrackItem track) async {
    // If it's already in the queue, jump to it
    final existingIdx = _queue.indexWhere((t) => t.mediaId == track.mediaId);
    if (existingIdx != -1) {
      _currentIndex = existingIdx;
    } else {
      // Insert immediately after current position
      final insertAt = (_currentIndex + 1).clamp(0, _queue.length);
      _queue.insert(insertAt, track);
      _currentIndex = insertAt;
    }
    
    // Keep originalQueue synced
    final origIdx = _originalQueue.indexWhere((t) => t.mediaId == track.mediaId);
    if (origIdx == -1) {
      final insertAtOrig = (_currentIndex).clamp(0, _originalQueue.length);
      _originalQueue.insert(insertAtOrig, track);
    }
    
    _isRadioActive = true;
    _error = null;
    notifyListeners();
    await _playCurrent();
    _checkAutoFetch();
  }

  void addTrack(TrackItem track) {
    // If nothing is playing, start playing
    if (_queue.isEmpty) {
      _queue.add(track);
      _currentIndex = 0;
      _isRadioActive = true;
      _error = null;
      notifyListeners();
      _playCurrent();
    } else {
      // Insert after current to play next
      final insertAt = (_currentIndex + 1).clamp(0, _queue.length);
      _queue.insert(insertAt, track);
      _error = null;
      notifyListeners();
      // If nothing playing, start from inserted
      if (!_isPlaying && !_isBuffering) {
        _currentIndex = insertAt;
        _playCurrent();
      }
    }
    _checkAutoFetch();
  }

  void startRadio(TrackItem seed) {
    _isRadioActive = true;
    setQueue([seed]);
  }

  Future<void> searchAndStartRadio(String query) async {
    _isBuffering = true;
    _error = null;
    notifyListeners();

    try {
      final results = await _youtubeFetcher.searchTracks(query);
      if (results.isNotEmpty) {
        final first = results.first;
        final seed = TrackItem(
          title: first.title,
          artist: first.author,
          mediaId: first.id.value,
          thumbnailUrl: first.thumbnails.highResUrl,
          duration: first.duration,
        );
        startRadio(seed);
      } else {
        _error = 'No results found';
        _isBuffering = false;
        notifyListeners();
      }
    } catch (e) {
      _error = 'Search failed: $e';
      _isBuffering = false;
      notifyListeners();
    }
  }

  Future<void> _checkAutoFetch() async {
    if (!_isRadioActive || _isFetchingRelated) return;

    final upcomingCount = _queue.length - 1 - _currentIndex;
    if (upcomingCount <= 2) {
      _isFetchingRelated = true;

      final current = currentTrack;
      if (current != null) {
        try {
          final relatedVideos =
              await _youtubeFetcher.getRelatedTracks(
                current.mediaId,
                title: current.title,
                artist: current.artist,
              );

          final existingIds = _queue.map((t) => t.mediaId).toSet();
          final newTracks = relatedVideos
              .where((v) => !existingIds.contains(v.id.value))
              .map((v) => TrackItem(
                    title: v.title,
                    artist: v.author,
                    mediaId: v.id.value,
                    thumbnailUrl: v.thumbnails.highResUrl,
                    duration: v.duration,
                  ))
              .toList();

          // Sort enqueued tracks by user taste preference score
          newTracks.sort((a, b) {
            final scoreA = MeduzaIntelligenceEngine.scoreTrack(title: a.title, artist: a.artist, mediaId: a.mediaId);
            final scoreB = MeduzaIntelligenceEngine.scoreTrack(title: b.title, artist: b.artist, mediaId: b.mediaId);
            return scoreB.compareTo(scoreA); // descending
          });

          if (newTracks.isNotEmpty) {
            _queue.addAll(newTracks);
            if (_isShuffling) {
              intelligentShuffle();
            }
          }
        } catch (e) {
          debugPrint('[PlaybackManager] Auto-fetch error: $e');
        }
      }

      _isFetchingRelated = false;
      notifyListeners();
    }
  }

  void intelligentShuffle() {
    if (_queue.length <= 1) return;
    if (_currentIndex >= _queue.length - 1) return;

    final upcoming = _queue.sublist(_currentIndex + 1);
    final shuffledIndices = MeduzaIntelligenceEngine.shuffleWithIntelligence(
      items: upcoming,
    );

    final shuffledUpcoming = shuffledIndices.map((i) => upcoming[i]).toList();

    _queue = [
      ..._queue.sublist(0, _currentIndex + 1),
      ...shuffledUpcoming,
    ];
    notifyListeners();
  }

  Future<void> _playCurrent() async {
    final track = currentTrack;
    if (track == null) return;

    // Increment generation - any older resolution will bail out
    final myGeneration = ++_playGeneration;

    _isBuffering = true;
    _error = null;
    notifyListeners();

    // Color extraction via hash
    currentHue = (track.mediaId.hashCode.abs() % 360).toDouble();

    try {
      final streamUrl = await _youtubeFetcher.getAudioStreamUrl(track.mediaId);

      // Bail out if a newer play request has superseded this one
      if (myGeneration != _playGeneration) {
        debugPrint('[PlaybackManager] Skipped stale request for ${track.mediaId}');
        return;
      }

      if (streamUrl != null) {
        final media = Media(streamUrl, httpHeaders: {
          'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36',
          'Referer': 'https://www.youtube.com/',
          'Origin': 'https://www.youtube.com',
        });

        // Record feedback for previous track before opening the next
        _recordFeedbackForActiveTrack();

        await _player.stop();
        await _player.open(media, play: true);

        // Start tracking feedback for the new active track
        _activePlayingTrack = track;
        _activeTrackStartTime = DateTime.now();

        // Feed to local caches too
        recentlyPlayedIds.add(track.mediaId);
        recentlyPlayed.removeWhere((t) => t.mediaId == track.mediaId);
        recentlyPlayed.insert(0, track);
        if (recentlyPlayed.length > 20) {
          recentlyPlayed.removeLast();
        }

        // Intelligence: Pre-fetch next stream for instantaneous switching
        if (_currentIndex + 1 < _queue.length) {
          _youtubeFetcher.getAudioStreamUrl(_queue[_currentIndex + 1].mediaId).catchError((_) => null);
        }
      } else {
        if (myGeneration == _playGeneration) {
          _error = 'Could not load audio stream';
          _isPlaying = false;
          debugPrint('[PlaybackManager] Null stream URL for ${track.mediaId}');
        }
      }
    } catch (e) {
      if (myGeneration == _playGeneration) {
        _error = 'Playback error: ${e.toString().split('\n').first}';
        _isPlaying = false;
        debugPrint('[PlaybackManager] _playCurrent error: $e');
      }
    }

    if (myGeneration == _playGeneration) {
      _isBuffering = false;
      notifyListeners();
    }
  }

  @override
  void dispose() {
    _recordFeedbackForActiveTrack();
    _positionSubscription?.cancel();
    _durationSubscription?.cancel();
    _playingSubscription?.cancel();
    _bufferingSubscription?.cancel();
    _completedSubscription?.cancel();
    _errorSubscription?.cancel();
    _volumeSubscription?.cancel();
    _youtubeFetcher.dispose();
    _player.dispose();
    super.dispose();
  }
}
