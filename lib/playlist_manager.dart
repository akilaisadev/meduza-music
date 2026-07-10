import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';
import 'intelligence_engine.dart';

class Playlist {
  final String id;
  String name;
  final List<TrackItem> tracks;

  Playlist({required this.id, required this.name, required this.tracks});

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'tracks': tracks.map((t) => {
        'title': t.title,
        'artist': t.artist,
        'mediaId': t.mediaId,
        'thumbnailUrl': t.thumbnailUrl,
        'durationMs': t.duration?.inMilliseconds,
      }).toList(),
    };
  }

  factory Playlist.fromJson(Map<String, dynamic> json) {
    return Playlist(
      id: json['id'],
      name: json['name'],
      tracks: (json['tracks'] as List).map((t) {
        return TrackItem(
          title: t['title'] ?? 'Unknown Title',
          artist: t['artist'] ?? 'Unknown Artist',
          mediaId: t['mediaId'] ?? '',
          thumbnailUrl: t['thumbnailUrl'] ?? '',
          duration: t['durationMs'] != null ? Duration(milliseconds: t['durationMs']) : null,
        );
      }).toList(),
    );
  }
}

class PlaylistManager extends ChangeNotifier {
  List<Playlist> _playlists = [];

  PlaylistManager() {
    loadPlaylists();
  }

  List<Playlist> get playlists => _playlists;

  File _getPlaylistFile() {
    final home = Platform.environment['HOME'];
    if (home != null) {
      final dir = Directory('$home/.config/meduza');
      if (!dir.existsSync()) {
        dir.createSync(recursive: true);
      }
      return File('${dir.path}/playlists.json');
    }
    return File('playlists.json');
  }

  Future<void> loadPlaylists() async {
    try {
      final file = _getPlaylistFile();
      if (await file.exists()) {
        final content = await file.readAsString();
        final List<dynamic> jsonList = jsonDecode(content);
        _playlists = jsonList.map((item) => Playlist.fromJson(item)).toList();
      } else {
        // Create a default playlist to start with
        _playlists = [
          Playlist(
            id: 'liked',
            name: 'Liked Songs',
            tracks: [],
          )
        ];
        await savePlaylists();
      }
    } catch (e) {
      debugPrint('[PlaylistManager] Error loading playlists: $e');
    }
    notifyListeners();
  }

  Future<void> savePlaylists() async {
    try {
      final file = _getPlaylistFile();
      final content = jsonEncode(_playlists.map((p) => p.toJson()).toList());
      await file.writeAsString(content);
    } catch (e) {
      debugPrint('[PlaylistManager] Error saving playlists: $e');
    }
  }

  Future<void> createPlaylist(String name) async {
    final id = DateTime.now().millisecondsSinceEpoch.toString();
    _playlists.add(Playlist(id: id, name: name, tracks: []));
    await savePlaylists();
    notifyListeners();
  }

  Future<void> deletePlaylist(String id) async {
    _playlists.removeWhere((p) => p.id == id);
    await savePlaylists();
    notifyListeners();
  }

  Future<void> renamePlaylist(String id, String newName) async {
    final idx = _playlists.indexWhere((p) => p.id == id);
    if (idx != -1) {
      _playlists[idx].name = newName;
      await savePlaylists();
      notifyListeners();
    }
  }

  Future<void> addTrackToPlaylist(String playlistId, TrackItem track) async {
    final idx = _playlists.indexWhere((p) => p.id == playlistId);
    if (idx != -1) {
      // Avoid duplicate tracks in the same playlist
      if (!_playlists[idx].tracks.any((t) => t.mediaId == track.mediaId)) {
        _playlists[idx].tracks.add(track);
        await savePlaylists();
        notifyListeners();
      }
    }
  }

  Future<void> removeTrackFromPlaylist(String playlistId, String mediaId) async {
    final idx = _playlists.indexWhere((p) => p.id == playlistId);
    if (idx != -1) {
      _playlists[idx].tracks.removeWhere((t) => t.mediaId == mediaId);
      await savePlaylists();
      notifyListeners();
    }
  }
}
