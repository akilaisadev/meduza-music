import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:youtube_explode_dart/youtube_explode_dart.dart';
import 'dart:async';
import 'intelligence_engine.dart';
import 'playback_manager.dart';
import 'youtube_fetcher.dart';
import 'playlist_manager.dart';
import 'theme_engine.dart';

class SearchView extends StatefulWidget {
  const SearchView({super.key});

  @override
  State<SearchView> createState() => _SearchViewState();
}

class Debouncer {
  final int milliseconds;
  Timer? _timer;
  Debouncer({required this.milliseconds});
  void run(VoidCallback action) {
    if (_timer?.isActive ?? false) _timer!.cancel();
    _timer = Timer(Duration(milliseconds: milliseconds), action);
  }

  void dispose() {
    _timer?.cancel();
  }
}

class _SearchViewState extends State<SearchView> {
  final TextEditingController _controller = TextEditingController();
  final YouTubeFetcher _fetcher = YouTubeFetcher();
  final Debouncer _debouncer = Debouncer(milliseconds: 350);

  bool _isLoading = false;
  List<Video> _results = [];
  String _lastQuery = '';

  @override
  void dispose() {
    _controller.dispose();
    _debouncer.dispose();
    _fetcher.dispose();
    super.dispose();
  }

  void _performSearch(String query) async {
    final trimmed = query.trim();
    if (trimmed.isEmpty || trimmed == _lastQuery) {
      if (trimmed.isEmpty) {
        setState(() {
          _results = [];
          _isLoading = false;
        });
      }
      return;
    }
    _lastQuery = trimmed;

    setState(() {
      _isLoading = true;
    });

    final results = await _fetcher.searchTracks(trimmed);

    if (mounted && trimmed == _lastQuery) {
      setState(() {
        _results = results;
        _isLoading = false;
      });
    }
  }

  void _playNow(Video video) {
    final track = TrackItem(
      title: video.title,
      artist: video.author,
      mediaId: video.id.value,
      thumbnailUrl: video.thumbnails.highResUrl,
      duration: video.duration,
    );
    context.read<PlaybackManager>().playNow(track);
  }

  void _queueTrack(Video video) {
    final track = TrackItem(
      title: video.title,
      artist: video.author,
      mediaId: video.id.value,
      thumbnailUrl: video.thumbnails.highResUrl,
      duration: video.duration,
    );
    context.read<PlaybackManager>().addTrack(track);
    final colors = context.read<ThemeState>().colors;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        backgroundColor: colors.surface,
        content: Text(
          'Added to queue: ${video.title}',
          style: TextStyle(color: colors.textPrimary),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        duration: const Duration(seconds: 2),
      ),
    );
  }

  void _showAddToPlaylistMenu(BuildContext context, Video video, MeduzaDynamicColors colors) {
    final playlistManager = context.read<PlaylistManager>();
    final playlists = playlistManager.playlists;

    final track = TrackItem(
      title: video.title,
      artist: video.author,
      mediaId: video.id.value,
      thumbnailUrl: video.thumbnails.highResUrl,
      duration: video.duration,
    );

    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          backgroundColor: colors.surfaceHigh,
          title: Text('Add to Playlist', style: TextStyle(color: colors.textPrimary)),
          content: SizedBox(
            width: 300,
            child: playlists.isEmpty
                ? Text('No playlists created yet.', style: TextStyle(color: colors.textSecondary))
                : ListView.builder(
                    shrinkWrap: true,
                    itemCount: playlists.length,
                    itemBuilder: (context, index) {
                      final p = playlists[index];
                      return ListTile(
                        leading: Icon(p.id == 'liked' ? Icons.favorite : Icons.music_note, color: colors.accent),
                        title: Text(p.name, style: TextStyle(color: colors.textPrimary)),
                        onTap: () {
                          playlistManager.addTrackToPlaylist(p.id, track);
                          Navigator.pop(context);
                          ScaffoldMessenger.of(context).showSnackBar(
                            SnackBar(
                              backgroundColor: colors.surface,
                              content: Text('Added to ${p.name}', style: TextStyle(color: colors.textPrimary)),
                              duration: const Duration(seconds: 2),
                            ),
                          );
                        },
                      );
                    },
                  ),
          ),
        );
      },
    );
  }

  String _formatDuration(Duration? d) {
    if (d == null) return '';
    final m = d.inMinutes.remainder(60).toString().padLeft(2, '0');
    final s = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    return '$m:$s';
  }

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;
    final pm = context.watch<PlaybackManager>();

    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1200),
        child: Padding(
          padding: const EdgeInsets.all(32.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Search',
                style: TextStyle(
                  fontSize: 40,
                  fontWeight: FontWeight.w800,
                  color: colors.textPrimary,
                  letterSpacing: -1.5,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                'Find and instantly play any song, album, or artist',
                style: TextStyle(fontSize: 16, color: colors.textSecondary),
              ),
              const SizedBox(height: 24),

              // Search Bar
              TextField(
                controller: _controller,
                style: TextStyle(color: colors.textPrimary, fontSize: 17),
                autofocus: false,
                onChanged: (val) => _debouncer.run(() => _performSearch(val)),
                onSubmitted: _performSearch,
                decoration: InputDecoration(
                  hintText: 'What do you want to listen to?',
                  hintStyle: TextStyle(color: colors.textSecondary.withOpacity(0.5)),
                  prefixIcon: Icon(Icons.search, color: colors.textSecondary),
                  suffixIcon: _controller.text.isNotEmpty
                      ? IconButton(
                          icon: Icon(Icons.close, color: colors.textSecondary),
                          onPressed: () {
                            _controller.clear();
                            setState(() {
                              _results = [];
                              _lastQuery = '';
                            });
                          },
                        )
                      : null,
                  filled: true,
                  fillColor: colors.surfaceHigh,
                  contentPadding: const EdgeInsets.symmetric(vertical: 18),
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(16),
                    borderSide: BorderSide.none,
                  ),
                  focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(16),
                    borderSide: BorderSide(color: colors.accentGlow, width: 2),
                  ),
                ),
              ),

              const SizedBox(height: 24),

              // Results
              Expanded(
                child: _isLoading
                    ? Center(child: CircularProgressIndicator(color: colors.accent))
                    : _results.isEmpty
                        ? Center(
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(Icons.queue_music, size: 72, color: colors.textSecondary.withOpacity(0.15)),
                                const SizedBox(height: 20),
                                Text(
                                  'Search YouTube Music',
                                  style: TextStyle(color: colors.textSecondary, fontSize: 18),
                                ),
                                const SizedBox(height: 8),
                                Text(
                                  'Results appear as you type',
                                  style: TextStyle(color: colors.textSecondary.withOpacity(0.5), fontSize: 14),
                                ),
                              ],
                            ),
                          )
                        : ListView.builder(
                            itemCount: _results.length,
                            itemBuilder: (context, index) {
                              final video = _results[index];
                              final isCurrentlyPlaying = pm.currentTrack?.mediaId == video.id.value;

                              return _SearchResultTile(
                                video: video,
                                colors: colors,
                                isCurrentlyPlaying: isCurrentlyPlaying,
                                duration: _formatDuration(video.duration),
                                onPlay: () => _playNow(video),
                                onQueue: () => _queueTrack(video),
                                onAddToPlaylist: () => _showAddToPlaylistMenu(context, video, colors),
                              );
                            },
                          ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SearchResultTile extends StatefulWidget {
  final Video video;
  final MeduzaDynamicColors colors;
  final bool isCurrentlyPlaying;
  final String duration;
  final VoidCallback onPlay;
  final VoidCallback onQueue;
  final VoidCallback onAddToPlaylist;

  const _SearchResultTile({
    required this.video,
    required this.colors,
    required this.isCurrentlyPlaying,
    required this.duration,
    required this.onPlay,
    required this.onQueue,
    required this.onAddToPlaylist,
  });

  @override
  State<_SearchResultTile> createState() => _SearchResultTileState();
}

class _SearchResultTileState extends State<_SearchResultTile> {
  bool _isHovered = false;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() => _isHovered = false),
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: widget.onPlay,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 150),
          margin: const EdgeInsets.only(bottom: 6),
          decoration: BoxDecoration(
            color: widget.isCurrentlyPlaying
                ? widget.colors.accent.withOpacity(0.12)
                : _isHovered
                    ? widget.colors.surfaceHigh
                    : Colors.transparent,
            borderRadius: BorderRadius.circular(10),
            border: widget.isCurrentlyPlaying
                ? Border.all(color: widget.colors.accent.withOpacity(0.3))
                : null,
          ),
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(
            children: [
              // Thumbnail
              ClipRRect(
                borderRadius: BorderRadius.circular(6),
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    Image.network(
                      widget.video.thumbnails.highResUrl,
                      width: 56,
                      height: 56,
                      fit: BoxFit.cover,
                      errorBuilder: (_, __, ___) =>
                          Container(width: 56, height: 56, color: widget.colors.surfaceHigh),
                    ),
                    if (widget.isCurrentlyPlaying)
                      Container(
                        width: 56,
                        height: 56,
                        color: Colors.black38,
                        child: Icon(Icons.graphic_eq, color: widget.colors.accent, size: 24),
                      ),
                    if (!widget.isCurrentlyPlaying && _isHovered)
                      Container(
                        width: 56,
                        height: 56,
                        color: Colors.black38,
                        child: const Icon(Icons.play_arrow, color: Colors.white, size: 28),
                      ),
                  ],
                ),
              ),
              const SizedBox(width: 16),
              // Title + Artist
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      widget.video.title,
                      style: TextStyle(
                        color: widget.isCurrentlyPlaying ? widget.colors.accent : widget.colors.textPrimary,
                        fontWeight: widget.isCurrentlyPlaying ? FontWeight.bold : FontWeight.normal,
                        fontSize: 15,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      widget.video.author,
                      style: TextStyle(color: widget.colors.textSecondary, fontSize: 12),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),
              // Duration
              if (widget.duration.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: Text(
                    widget.duration,
                    style: TextStyle(color: widget.colors.textSecondary, fontSize: 12),
                  ),
                ),
              // Action buttons (visible on hover or currently playing)
              if (_isHovered || widget.isCurrentlyPlaying) ...[
                IconButton(
                  icon: Icon(Icons.playlist_add, color: widget.colors.textSecondary, size: 20),
                  tooltip: 'Add to playlist',
                  onPressed: widget.onAddToPlaylist,
                ),
                IconButton(
                  icon: Icon(Icons.queue_music, color: widget.colors.textSecondary, size: 20),
                  tooltip: 'Add to queue',
                  onPressed: widget.onQueue,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
