import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'dart:ui';
import 'theme_engine.dart';
import 'playback_manager.dart';
import 'intelligence_engine.dart';
import 'youtube_fetcher.dart';
import 'playlist_manager.dart';

class PlaylistView extends StatefulWidget {
  final String playlistId;
  final bool isLocal;
  
  const PlaylistView({super.key, required this.playlistId, this.isLocal = false});

  @override
  State<PlaylistView> createState() => _PlaylistViewState();
}

class _PlaylistViewState extends State<PlaylistView> {
  final YouTubeFetcher _fetcher = YouTubeFetcher();
  Map<String, dynamic>? _playlistData;
  bool _isLoading = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _loadPlaylist();
  }

  Future<void> _loadPlaylist() async {
    if (widget.isLocal) {
      final pm = context.read<PlaylistManager>();
      // Use helper to safely extract local playlist
      final idx = pm.playlists.indexWhere((p) => p.id == widget.playlistId);
      if (idx != -1) {
        final localPlaylist = pm.playlists[idx];
        setState(() {
          _playlistData = {
            'title': localPlaylist.name,
            'author': 'You',
            'thumbnail': localPlaylist.tracks.isNotEmpty ? localPlaylist.tracks.first.thumbnailUrl : '',
            'tracks': localPlaylist.tracks,
          };
          _isLoading = false;
        });
      } else {
        setState(() {
          _error = 'Local playlist not found';
          _isLoading = false;
        });
      }
      return;
    }

    // Remote playlist loading
    final data = await _fetcher.getPlaylistDetails(widget.playlistId);
    if (mounted) {
      setState(() {
        if (data != null) {
          final videos = data['videos'] as List<dynamic>;
          final tracks = videos.map((v) => TrackItem(
            title: v.title,
            artist: v.author,
            mediaId: v.id.value,
            thumbnailUrl: v.thumbnails.highResUrl,
            duration: v.duration,
          )).toList();
          _playlistData = {
            'title': data['title'],
            'author': data['author'],
            'thumbnail': data['thumbnail'],
            'tracks': tracks,
          };
        } else {
          _error = 'Failed to load playlist';
        }
        _isLoading = false;
      });
    }
  }

  void _playAll() {
    if (_playlistData == null) return;
    final tracks = _playlistData!['tracks'] as List<TrackItem>;
    if (tracks.isEmpty) return;
    context.read<PlaybackManager>().setQueue(tracks);
  }

  void _playTrack(int index) {
    if (_playlistData == null) return;
    final tracks = _playlistData!['tracks'] as List<TrackItem>;
    context.read<PlaybackManager>().setQueue(tracks, initialIndex: index);
  }

  String _formatDuration(Duration? d) {
    if (d == null) return '--:--';
    final minutes = d.inMinutes.remainder(60).toString().padLeft(2, '0');
    final seconds = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    if (d.inHours > 0) {
      return '${d.inHours}:$minutes:$seconds';
    }
    return '$minutes:$seconds';
  }

  void _deleteLocalPlaylist() {
    if (!widget.isLocal) return;
    showDialog(
      context: context,
      builder: (context) {
        final colors = context.read<ThemeState>().colors;
        return AlertDialog(
          backgroundColor: colors.surfaceHigh,
          title: Text('Delete Playlist', style: TextStyle(color: colors.textPrimary)),
          content: Text('Are you sure you want to delete "${_playlistData!['title']}"?', style: TextStyle(color: colors.textSecondary)),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text('Cancel', style: TextStyle(color: colors.textSecondary)),
            ),
            TextButton(
              onPressed: () {
                context.read<PlaylistManager>().deletePlaylist(widget.playlistId);
                Navigator.pop(context); // Close dialog
                Navigator.pop(context); // Close PlaylistView
              },
              child: const Text('Delete', style: TextStyle(color: Colors.redAccent)),
            ),
          ],
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;

    if (_isLoading) {
      return Scaffold(
        backgroundColor: Colors.transparent,
        body: Center(child: CircularProgressIndicator(color: colors.accent)),
      );
    }

    if (_error != null || _playlistData == null) {
      return Scaffold(
        backgroundColor: Colors.transparent,
        body: Center(child: Text(_error ?? 'Unknown error', style: TextStyle(color: colors.textPrimary))),
      );
    }

    // List of dynamic tracks
    final tracks = _playlistData!['tracks'] as List<TrackItem>;
    final hasCover = (_playlistData!['thumbnail'] as String).isNotEmpty;

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: CustomScrollView(
        slivers: [
          SliverAppBar(
            expandedHeight: 300,
            pinned: true,
            backgroundColor: colors.background.withOpacity(0.9),
            leading: IconButton(
              icon: Icon(Icons.arrow_back, color: colors.textPrimary),
              onPressed: () {
                Navigator.pop(context);
              },
            ),
            flexibleSpace: FlexibleSpaceBar(
              background: Stack(
                fit: StackFit.expand,
                children: [
                  // Blurred Background
                  if (hasCover)
                    Image.network(
                      _playlistData!['thumbnail'],
                      fit: BoxFit.cover,
                    )
                  else
                    Container(color: colors.surfaceHigh),
                  BackdropFilter(
                    filter: ImageFilter.blur(sigmaX: 50, sigmaY: 50),
                    child: Container(
                      color: colors.background.withOpacity(0.6),
                    ),
                  ),
                  // Hero Content
                  Positioned(
                    bottom: 24,
                    left: 24,
                    right: 24,
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        Container(
                          width: 180,
                          height: 180,
                          decoration: BoxDecoration(
                            boxShadow: [
                              BoxShadow(
                                color: Colors.black.withOpacity(0.5),
                                blurRadius: 20,
                                offset: const Offset(0, 10),
                              )
                            ],
                          ),
                          child: ClipRRect(
                            borderRadius: BorderRadius.circular(8),
                            child: hasCover
                                ? Image.network(
                                    _playlistData!['thumbnail'],
                                    fit: BoxFit.cover,
                                  )
                                : Container(
                                    color: colors.surface,
                                    child: Icon(
                                      widget.playlistId == 'liked' ? Icons.favorite : Icons.music_note,
                                      size: 80,
                                      color: colors.accentGlow.withOpacity(0.5),
                                    ),
                                  ),
                          ),
                        ),
                        const SizedBox(width: 24),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                widget.isLocal ? 'Custom Playlist' : 'YouTube Mix',
                                style: TextStyle(color: colors.textSecondary, fontSize: 14),
                              ),
                              const SizedBox(height: 8),
                              Text(
                                _playlistData!['title'],
                                style: TextStyle(
                                  color: colors.textPrimary,
                                  fontSize: 48,
                                  fontWeight: FontWeight.bold,
                                  height: 1.1,
                                ),
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                              ),
                              const SizedBox(height: 16),
                              Row(
                                children: [
                                  Text(
                                    _playlistData!['author'],
                                    style: TextStyle(color: colors.textPrimary, fontWeight: FontWeight.bold),
                                  ),
                                  const SizedBox(width: 8),
                                  Text(
                                    '• ${tracks.length} tracks',
                                    style: TextStyle(color: colors.textSecondary),
                                  ),
                                ],
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          
          // Play Button & Controls Bar
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.all(24.0),
              child: Row(
                children: [
                  Container(
                    width: 56,
                    height: 56,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      gradient: colors.accentGradient,
                      boxShadow: [
                        BoxShadow(
                          color: colors.accentGlow.withOpacity(0.5),
                          blurRadius: 15,
                          offset: const Offset(0, 5),
                        )
                      ],
                    ),
                    child: IconButton(
                      icon: const Icon(Icons.play_arrow, size: 32, color: Colors.white),
                      onPressed: _playAll,
                    ),
                  ),
                  if (widget.isLocal && widget.playlistId != 'liked') ...[
                    const SizedBox(width: 16),
                    IconButton(
                      icon: const Icon(Icons.delete_outline, color: Colors.redAccent, size: 28),
                      onPressed: _deleteLocalPlaylist,
                    ),
                  ],
                ],
              ),
            ),
          ),
          
          // Track List Header
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 40, vertical: 8),
              child: Row(
                children: [
                  SizedBox(width: 32, child: Text('#', style: TextStyle(color: colors.textSecondary))),
                  Expanded(child: Text('Title', style: TextStyle(color: colors.textSecondary))),
                  SizedBox(width: 60, child: Text('Time', style: TextStyle(color: colors.textSecondary), textAlign: TextAlign.right)),
                ],
              ),
            ),
          ),
          
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24),
              child: Divider(color: colors.border),
            ),
          ),

          // Track List
          SliverList(
            delegate: SliverChildBuilderDelegate(
              (context, index) {
                final track = tracks[index];
                return _TrackListItem(
                  index: index,
                  track: track,
                  colors: colors,
                  onTap: () => _playTrack(index),
                  duration: _formatDuration(track.duration),
                  isLocal: widget.isLocal,
                  playlistId: widget.playlistId,
                  onRemove: () {
                    // Trigger dynamic playlist update
                    setState(() {
                      tracks.removeAt(index);
                    });
                  },
                );
              },
              childCount: tracks.length,
            ),
          ),
          
          // Bottom padding
          const SliverToBoxAdapter(child: SizedBox(height: 40)),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _fetcher.dispose();
    super.dispose();
  }
}

class _TrackListItem extends StatefulWidget {
  final int index;
  final TrackItem track;
  final MeduzaDynamicColors colors;
  final VoidCallback onTap;
  final String duration;
  final bool isLocal;
  final String playlistId;
  final VoidCallback onRemove;

  const _TrackListItem({
    required this.index,
    required this.track,
    required this.colors,
    required this.onTap,
    required this.duration,
    required this.isLocal,
    required this.playlistId,
    required this.onRemove,
  });

  @override
  State<_TrackListItem> createState() => _TrackListItemState();
}

class _TrackListItemState extends State<_TrackListItem> {
  bool _isHovered = false;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() => _isHovered = false),
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: widget.onTap,
        child: Container(
          margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 2),
          decoration: BoxDecoration(
            color: _isHovered ? widget.colors.surfaceHigh : Colors.transparent,
            borderRadius: BorderRadius.circular(8),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
          child: Row(
            children: [
              SizedBox(
                width: 32,
                child: _isHovered 
                  ? Icon(Icons.play_arrow, color: widget.colors.textPrimary, size: 20)
                  : Text('${widget.index + 1}', style: TextStyle(color: widget.colors.textSecondary)),
              ),
              ClipRRect(
                borderRadius: BorderRadius.circular(4),
                child: widget.track.thumbnailUrl.isNotEmpty
                    ? Image.network(
                        widget.track.thumbnailUrl,
                        width: 40,
                        height: 40,
                        fit: BoxFit.cover,
                        errorBuilder: (context, error, stackTrace) =>
                            Container(width: 40, height: 40, color: widget.colors.surfaceHigh),
                      )
                    : Container(width: 40, height: 40, color: widget.colors.surfaceHigh),
              ),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      widget.track.title,
                      style: TextStyle(
                        color: widget.colors.textPrimary,
                        fontWeight: _isHovered ? FontWeight.bold : FontWeight.normal,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      widget.track.artist,
                      style: TextStyle(color: widget.colors.textSecondary, fontSize: 12),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),
              if (widget.isLocal && _isHovered)
                IconButton(
                  icon: const Icon(Icons.remove_circle_outline, color: Colors.redAccent, size: 20),
                  onPressed: () {
                    context.read<PlaylistManager>().removeTrackFromPlaylist(widget.playlistId, widget.track.mediaId);
                    widget.onRemove();
                  },
                ),
              const SizedBox(width: 16),
              SizedBox(
                width: 60,
                child: Text(
                  widget.duration,
                  style: TextStyle(color: widget.colors.textSecondary),
                  textAlign: TextAlign.right,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
