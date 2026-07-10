import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'dart:ui';
import 'theme_engine.dart';
import 'playback_manager.dart';
import 'playlist_manager.dart';

class FullScreenPlayerView extends StatelessWidget {
  const FullScreenPlayerView({super.key});

  String _formatDuration(Duration d) {
    final minutes = d.inMinutes.remainder(60).toString().padLeft(2, '0');
    final seconds = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    return '$minutes:$seconds';
  }

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;
    final pm = context.watch<PlaybackManager>();
    final playlistManager = context.watch<PlaylistManager>();

    final track = pm.currentTrack;
    if (track == null) {
      return Container(
        height: MediaQuery.of(context).size.height,
        color: colors.background,
        child: const Center(child: Text('Nothing Playing')),
      );
    }

    final isPlaying = pm.isPlaying;
    final isBuffering = pm.isBuffering;
    final isLooping = pm.isLooping;
    final isShuffling = pm.isShuffling;
    final pos = pm.position;
    final dur = pm.duration;
    final progress = dur.inMilliseconds > 0 ? pos.inMilliseconds / dur.inMilliseconds : 0.0;

    // Check if current track is in Liked Songs
    final isLiked = playlistManager.playlists.any(
      (p) => p.id == 'liked' && p.tracks.any((t) => t.mediaId == track.mediaId),
    );

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Stack(
        children: [
          // 1. Immersive Blur Background using the current album art / color hue
          Positioned.fill(
            child: track.thumbnailUrl.isNotEmpty
                ? Image.network(
                    track.thumbnailUrl,
                    fit: BoxFit.cover,
                  )
                : Container(color: colors.surfaceHigh),
          ),
          Positioned.fill(
            child: BackdropFilter(
              filter: ImageFilter.blur(sigmaX: 80, sigmaY: 80),
              child: Container(
                color: colors.background.withOpacity(0.78),
              ),
            ),
          ),

          // 2. Main Layout - Responsive Desktop (Split Page) vs Mobile (Single Column)
          SafeArea(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final isLandscape = constraints.maxWidth > 750;

                if (isLandscape) {
                  // Desktop Landscape Split Layout
                  return Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 60.0, vertical: 32.0),
                    child: Column(
                      children: [
                        // Top Bar: Dismiss & Header
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            IconButton(
                              icon: const Icon(Icons.keyboard_arrow_down, size: 36),
                              onPressed: () => Navigator.pop(context),
                            ),
                            Text(
                              'NOW PLAYING',
                              style: TextStyle(
                                fontSize: 13,
                                fontWeight: FontWeight.bold,
                                color: colors.textSecondary.withOpacity(0.8),
                                letterSpacing: 2.0,
                              ),
                            ),
                            const SizedBox(width: 48), // Spacer to balance back btn
                          ],
                        ),

                        const Spacer(),

                        // Horizontal Split
                        Row(
                          crossAxisAlignment: CrossAxisAlignment.center,
                          children: [
                            // Left Pane: Larger Spinning CD
                            Expanded(
                              flex: 5,
                              child: Center(
                                child: SpinningDisc(
                                  thumbnailUrl: track.thumbnailUrl,
                                  isPlaying: isPlaying,
                                  colors: colors,
                                  size: 340,
                                ),
                              ),
                            ),

                            const SizedBox(width: 64),

                            // Right Pane: Song Metadata & Controls
                            Expanded(
                              flex: 6,
                              child: Align(
                                alignment: Alignment.centerLeft,
                                child: ConstrainedBox(
                                  constraints: const BoxConstraints(maxWidth: 450),
                                  child: Column(
                                    mainAxisSize: MainAxisSize.min,
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      // Track details (Left Aligned for split panel)
                                      Text(
                                        track.title,
                                        style: const TextStyle(
                                          fontSize: 26,
                                          fontWeight: FontWeight.bold,
                                          overflow: TextOverflow.ellipsis,
                                        ),
                                        maxLines: 2,
                                      ),
                                      const SizedBox(height: 8),
                                      Text(
                                        track.artist,
                                        style: TextStyle(
                                          fontSize: 18,
                                          color: colors.textSecondary,
                                          overflow: TextOverflow.ellipsis,
                                        ),
                                        maxLines: 1,
                                      ),

                                      const SizedBox(height: 32),

                                      // Sleek Progress slider
                                      Column(
                                        children: [
                                          SliderTheme(
                                            data: SliderThemeData(
                                              trackHeight: 4,
                                              thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
                                              activeTrackColor: colors.accent,
                                              inactiveTrackColor: colors.surfaceHigh.withOpacity(0.3),
                                              overlayColor: colors.accent.withOpacity(0.15),
                                              trackShape: const RectangularSliderTrackShape(),
                                            ),
                                            child: Slider(
                                              value: progress.clamp(0.0, 1.0),
                                              onChanged: (val) {
                                                final newPos = Duration(
                                                    milliseconds: (val * dur.inMilliseconds).round());
                                                pm.seek(newPos);
                                              },
                                            ),
                                          ),
                                          Padding(
                                            padding: const EdgeInsets.symmetric(horizontal: 4.0),
                                            child: Row(
                                              mainAxisAlignment: MainAxisAlignment.spaceBetween,
                                              children: [
                                                Text(
                                                  _formatDuration(pos),
                                                  style: TextStyle(color: colors.textSecondary.withOpacity(0.8), fontSize: 12),
                                                ),
                                                Text(
                                                  _formatDuration(dur),
                                                  style: TextStyle(color: colors.textSecondary.withOpacity(0.8), fontSize: 12),
                                                ),
                                              ],
                                            ),
                                          ),
                                        ],
                                      ),

                                      const SizedBox(height: 24),

                                      // Primary Controls
                                      Row(
                                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                                        children: [
                                          IconButton(
                                            icon: const Icon(Icons.shuffle, size: 24),
                                            color: isShuffling ? colors.accentGlow : colors.textSecondary.withOpacity(0.5),
                                            onPressed: () => pm.toggleShuffle(),
                                          ),
                                          IconButton(
                                            icon: const Icon(Icons.skip_previous, size: 36),
                                            color: pm.hasPrevious ? colors.textPrimary : colors.textSecondary.withOpacity(0.2),
                                            onPressed: pm.hasPrevious ? () => pm.previous() : null,
                                          ),
                                          
                                          // Play Button
                                          GestureDetector(
                                            onTap: () => pm.playPause(),
                                            child: Container(
                                              width: 72,
                                              height: 72,
                                              decoration: BoxDecoration(
                                                shape: BoxShape.circle,
                                                gradient: LinearGradient(
                                                  colors: [colors.accent, colors.accentGlow],
                                                  begin: Alignment.topLeft,
                                                  end: Alignment.bottomRight,
                                                ),
                                                boxShadow: [
                                                  BoxShadow(
                                                    color: colors.accentGlow.withOpacity(0.35),
                                                    blurRadius: 18,
                                                    spreadRadius: 1,
                                                    offset: const Offset(0, 6),
                                                  )
                                                ],
                                              ),
                                              child: isBuffering
                                                  ? const Padding(
                                                      padding: EdgeInsets.all(24.0),
                                                      child: CircularProgressIndicator(
                                                        strokeWidth: 2,
                                                        color: Colors.white,
                                                      ),
                                                    )
                                                  : Icon(
                                                      isPlaying ? Icons.pause : Icons.play_arrow,
                                                      color: Colors.white,
                                                      size: 34,
                                                    ),
                                            ),
                                          ),
                                          
                                          IconButton(
                                            icon: const Icon(Icons.skip_next, size: 36),
                                            color: pm.hasNext ? colors.textPrimary : colors.textSecondary.withOpacity(0.2),
                                            onPressed: pm.hasNext ? () => pm.next() : null,
                                          ),
                                          IconButton(
                                            icon: Icon(isLooping ? Icons.repeat_one : Icons.repeat, size: 24),
                                            color: isLooping ? colors.accentGlow : colors.textSecondary.withOpacity(0.5),
                                            onPressed: () => pm.toggleLoop(),
                                          ),
                                        ],
                                      ),

                                      const SizedBox(height: 32),

                                      // Bottom Options Row
                                      Row(
                                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                                        children: [
                                          IconButton(
                                            icon: Icon(
                                              isLiked ? Icons.favorite : Icons.favorite_border,
                                              color: isLiked ? colors.accent : colors.textSecondary,
                                              size: 28,
                                            ),
                                            onPressed: () {
                                              if (isLiked) {
                                                playlistManager.removeTrackFromPlaylist('liked', track.mediaId);
                                              } else {
                                                playlistManager.addTrackToPlaylist('liked', track);
                                              }
                                            },
                                          ),
                                          
                                          // Volume Bar
                                          Expanded(
                                            child: Padding(
                                              padding: const EdgeInsets.only(left: 16.0, right: 32.0),
                                              child: Row(
                                                children: [
                                                  Icon(
                                                    pm.volume == 0
                                                        ? Icons.volume_off
                                                        : pm.volume < 50
                                                            ? Icons.volume_down
                                                            : Icons.volume_up,
                                                    color: colors.textSecondary.withOpacity(0.7),
                                                    size: 18,
                                                  ),
                                                  const SizedBox(width: 8),
                                                  Expanded(
                                                    child: SliderTheme(
                                                      data: SliderThemeData(
                                                        trackHeight: 3,
                                                        thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 0),
                                                        activeTrackColor: colors.textPrimary.withOpacity(0.9),
                                                        inactiveTrackColor: colors.surfaceHigh.withOpacity(0.3),
                                                      ),
                                                      child: Slider(
                                                        value: pm.volume / 100.0,
                                                        onChanged: (val) {
                                                          pm.setVolume(val * 100.0);
                                                        },
                                                      ),
                                                    ),
                                                  ),
                                                ],
                                              ),
                                            ),
                                          ),
                                        ],
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                            ),
                          ],
                        ),

                        const Spacer(),
                      ],
                    ),
                  );
                } else {
                  // Mobile / Portrait narrow layout (Centered Single Column)
                  return Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 24.0, vertical: 24.0),
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        // Top Bar
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            IconButton(
                              icon: const Icon(Icons.keyboard_arrow_down, size: 36),
                              onPressed: () => Navigator.pop(context),
                            ),
                            Text(
                              'NOW PLAYING',
                              style: TextStyle(
                                fontSize: 13,
                                fontWeight: FontWeight.bold,
                                color: colors.textSecondary.withOpacity(0.8),
                                letterSpacing: 2.0,
                              ),
                            ),
                            const SizedBox(width: 48),
                          ],
                        ),

                        const SizedBox(height: 16),

                        SpinningDisc(
                          thumbnailUrl: track.thumbnailUrl,
                          isPlaying: isPlaying,
                          colors: colors,
                          size: 280,
                        ),

                        const SizedBox(height: 24),

                        Column(
                          children: [
                            Text(
                              track.title,
                              textAlign: TextAlign.center,
                              style: const TextStyle(
                                fontSize: 22,
                                fontWeight: FontWeight.bold,
                                overflow: TextOverflow.ellipsis,
                              ),
                              maxLines: 1,
                            ),
                            const SizedBox(height: 6),
                            Text(
                              track.artist,
                              textAlign: TextAlign.center,
                              style: TextStyle(
                                fontSize: 16,
                                color: colors.textSecondary,
                                overflow: TextOverflow.ellipsis,
                              ),
                              maxLines: 1,
                            ),
                          ],
                        ),

                        const SizedBox(height: 16),

                        Column(
                          children: [
                            SliderTheme(
                              data: SliderThemeData(
                                trackHeight: 4,
                                thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
                                activeTrackColor: colors.accent,
                                inactiveTrackColor: colors.surfaceHigh.withOpacity(0.3),
                                overlayColor: colors.accent.withOpacity(0.15),
                                trackShape: const RectangularSliderTrackShape(),
                              ),
                              child: Slider(
                                value: progress.clamp(0.0, 1.0),
                                onChanged: (val) {
                                  final newPos = Duration(
                                      milliseconds: (val * dur.inMilliseconds).round());
                                  pm.seek(newPos);
                                },
                              ),
                            ),
                            Padding(
                              padding: const EdgeInsets.symmetric(horizontal: 4.0),
                              child: Row(
                                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                                children: [
                                  Text(
                                    _formatDuration(pos),
                                    style: TextStyle(color: colors.textSecondary.withOpacity(0.8), fontSize: 12),
                                  ),
                                  Text(
                                    _formatDuration(dur),
                                    style: TextStyle(color: colors.textSecondary.withOpacity(0.8), fontSize: 12),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),

                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            IconButton(
                              icon: const Icon(Icons.shuffle, size: 22),
                              color: isShuffling ? colors.accentGlow : colors.textSecondary.withOpacity(0.5),
                              onPressed: () => pm.toggleShuffle(),
                            ),
                            IconButton(
                              icon: const Icon(Icons.skip_previous, size: 32),
                              color: pm.hasPrevious ? colors.textPrimary : colors.textSecondary.withOpacity(0.2),
                              onPressed: pm.hasPrevious ? () => pm.previous() : null,
                            ),
                            GestureDetector(
                              onTap: () => pm.playPause(),
                              child: Container(
                                width: 68,
                                height: 68,
                                decoration: BoxDecoration(
                                  shape: BoxShape.circle,
                                  gradient: LinearGradient(
                                    colors: [colors.accent, colors.accentGlow],
                                    begin: Alignment.topLeft,
                                    end: Alignment.bottomRight,
                                  ),
                                  boxShadow: [
                                    BoxShadow(
                                      color: colors.accentGlow.withOpacity(0.35),
                                      blurRadius: 18,
                                      spreadRadius: 1,
                                      offset: const Offset(0, 6),
                                    )
                                  ],
                                ),
                                child: isBuffering
                                    ? const Padding(
                                        padding: EdgeInsets.all(22.0),
                                        child: CircularProgressIndicator(
                                          strokeWidth: 2,
                                          color: Colors.white,
                                        ),
                                      )
                                    : Icon(
                                        isPlaying ? Icons.pause : Icons.play_arrow,
                                        color: Colors.white,
                                        size: 32,
                                      ),
                              ),
                            ),
                            IconButton(
                              icon: const Icon(Icons.skip_next, size: 32),
                              color: pm.hasNext ? colors.textPrimary : colors.textSecondary.withOpacity(0.2),
                              onPressed: pm.hasNext ? () => pm.next() : null,
                            ),
                            IconButton(
                              icon: Icon(isLooping ? Icons.repeat_one : Icons.repeat, size: 22),
                              color: isLooping ? colors.accentGlow : colors.textSecondary.withOpacity(0.5),
                              onPressed: () => pm.toggleLoop(),
                            ),
                          ],
                        ),

                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            IconButton(
                              icon: Icon(
                                isLiked ? Icons.favorite : Icons.favorite_border,
                                color: isLiked ? colors.accent : colors.textSecondary,
                                size: 26,
                              ),
                              onPressed: () {
                                if (isLiked) {
                                  playlistManager.removeTrackFromPlaylist('liked', track.mediaId);
                                } else {
                                  playlistManager.addTrackToPlaylist('liked', track);
                                }
                              },
                            ),
                            Expanded(
                              child: Padding(
                                padding: const EdgeInsets.symmetric(horizontal: 16.0),
                                child: Row(
                                  children: [
                                    Icon(
                                      pm.volume == 0
                                          ? Icons.volume_off
                                          : pm.volume < 50
                                              ? Icons.volume_down
                                              : Icons.volume_up,
                                      color: colors.textSecondary.withOpacity(0.7),
                                      size: 16,
                                    ),
                                    const SizedBox(width: 8),
                                    Expanded(
                                      child: SliderTheme(
                                        data: SliderThemeData(
                                          trackHeight: 3,
                                          thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 0),
                                          activeTrackColor: colors.textPrimary.withOpacity(0.9),
                                          inactiveTrackColor: colors.surfaceHigh.withOpacity(0.3),
                                        ),
                                        child: Slider(
                                          value: pm.volume / 100.0,
                                          onChanged: (val) {
                                            pm.setVolume(val * 100.0);
                                          },
                                        ),
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                            const SizedBox(width: 36),
                          ],
                        ),
                      ],
                    ),
                  );
                }
              },
            ),
          ),
        ],
      ),
    );
  }
}

class SpinningDisc extends StatefulWidget {
  final String thumbnailUrl;
  final bool isPlaying;
  final MeduzaDynamicColors colors;
  final double size;

  const SpinningDisc({
    super.key,
    required this.thumbnailUrl,
    required this.isPlaying,
    required this.colors,
    this.size = 300,
  });

  @override
  State<SpinningDisc> createState() => _SpinningDiscState();
}

class _SpinningDiscState extends State<SpinningDisc> with SingleTickerProviderStateMixin {
  late AnimationController _rotationController;

  @override
  void initState() {
    super.initState();
    _rotationController = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 20),
    );
    if (widget.isPlaying) {
      _rotationController.repeat();
    }
  }

  @override
  void didUpdateWidget(covariant SpinningDisc oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.isPlaying != oldWidget.isPlaying) {
      if (widget.isPlaying) {
        _rotationController.repeat();
      } else {
        _rotationController.stop();
      }
    }
  }

  @override
  void dispose() {
    _rotationController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _rotationController,
      builder: (context, child) {
        return Transform.rotate(
          angle: _rotationController.value * 2 * 3.141592653589793,
          child: child,
        );
      },
      child: Container(
        width: widget.size,
        height: widget.size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          boxShadow: [
            BoxShadow(
              color: Colors.black.withOpacity(0.4),
              blurRadius: 30,
              spreadRadius: 2,
              offset: const Offset(0, 10),
            )
          ],
        ),
        child: ClipOval(
          child: Container(
            color: Colors.black,
            padding: const EdgeInsets.all(6.0), // Outer vinyl lip
            child: ClipOval(
              child: Stack(
                alignment: Alignment.center,
                children: [
                  widget.thumbnailUrl.isNotEmpty
                      ? Image.network(
                          widget.thumbnailUrl,
                          fit: BoxFit.cover,
                          width: double.infinity,
                          height: double.infinity,
                        )
                      : const Icon(Icons.music_note, size: 80, color: Colors.white),
                  // Vinyl disc texture lines
                  Container(
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(
                        color: Colors.black.withOpacity(0.45),
                        width: 1.5,
                      ),
                    ),
                  ),
                  Container(
                    width: widget.size * 0.66,
                    height: widget.size * 0.66,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(
                        color: Colors.black.withOpacity(0.35),
                        width: 1.0,
                      ),
                    ),
                  ),
                  // Center spindle hole
                  Container(
                    width: 36,
                    height: 36,
                    decoration: const BoxDecoration(
                      color: Colors.black,
                      shape: BoxShape.circle,
                    ),
                    child: Center(
                      child: Container(
                        width: 14,
                        height: 14,
                        decoration: BoxDecoration(
                          color: widget.colors.background,
                          shape: BoxShape.circle,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
