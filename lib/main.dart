import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';
import 'package:media_kit/media_kit.dart' hide Playlist;
import 'package:provider/provider.dart';
import 'dart:ui';
import 'theme_engine.dart';
import 'discover_view.dart';
import 'search_view.dart';
import 'playback_manager.dart';
import 'playlist_manager.dart';
import 'library_view.dart';
import 'settings_view.dart';
import 'full_screen_player_view.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Initialize media_kit
  MediaKit.ensureInitialized();

  // Initialize window_manager
  await windowManager.ensureInitialized();

  WindowOptions windowOptions = const WindowOptions(
    size: Size(1100, 700),
    center: true,
    backgroundColor: Colors.transparent,
    skipTaskbar: false,
    titleBarStyle: TitleBarStyle.hidden,
  );

  await windowManager.waitUntilReadyToShow(windowOptions, () async {
    await windowManager.show();
    await windowManager.focus();
  });

  runApp(
    MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => ThemeState()),
        ChangeNotifierProvider(create: (_) => PlaybackManager()),
        ChangeNotifierProvider(create: (_) => PlaylistManager()),
      ],
      child: const MeduzaApp(),
    ),
  );
}



class MeduzaApp extends StatelessWidget {
  const MeduzaApp({super.key});

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;

    return MaterialApp(
      title: 'Meduza Music',
      debugShowCheckedModeBanner: false,
      theme: ThemeData.dark().copyWith(
        scaffoldBackgroundColor: colors.background,
        colorScheme: ColorScheme.dark(
          primary: colors.accent,
          surface: colors.surface,
          secondary: colors.complement,
        ),
      ),
      home: const MainScreen(),
    );
  }
}

class MainScreen extends StatefulWidget {
  const MainScreen({super.key});

  @override
  State<MainScreen> createState() => _MainScreenState();
}

class _MainScreenState extends State<MainScreen> {
  int _selectedIndex = 0;
  double? _lastHue;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<PlaybackManager>().addListener(_onPlaybackChanged);
    });
  }

  void _onPlaybackChanged() {
    if (!mounted) return;
    final pm = context.read<PlaybackManager>();
    if (pm.currentHue != null && pm.currentHue != _lastHue) {
      _lastHue = pm.currentHue;
      context.read<ThemeState>().setHue(_lastHue!);
    }
  }

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;
    final pm = context.watch<PlaybackManager>();
    
    // Sync liked songs to intelligence engine
    final playlistManager = context.watch<PlaylistManager>();
    final liked = playlistManager.playlists.firstWhere(
      (p) => p.id == 'liked',
      orElse: () => Playlist(id: 'liked', name: 'Liked Songs', tracks: []),
    );
    pm.updateLikedTrackIds(liked.tracks.map((t) => t.mediaId).toSet());
    
    return Scaffold(
      body: Stack(
        children: [
          // Dynamic Album Art Blurred Background Layer with cross-fade
          Positioned.fill(
            child: Consumer<PlaybackManager>(
              builder: (context, pm, child) {
                final track = pm.currentTrack;
                return AnimatedSwitcher(
                  duration: const Duration(milliseconds: 600),
                  child: track != null && track.thumbnailUrl.isNotEmpty
                      ? Image.network(
                          track.thumbnailUrl,
                          key: ValueKey(track.mediaId),
                          fit: BoxFit.cover,
                          width: double.infinity,
                          height: double.infinity,
                        )
                      : Container(
                          key: const ValueKey('default'),
                          decoration: BoxDecoration(
                            gradient: RadialGradient(
                              center: const Alignment(-0.5, -0.5),
                              radius: 1.5,
                              colors: [
                                colors.surface,
                                colors.background,
                              ],
                            ),
                          ),
                        ),
                );
              },
            ),
          ),
          Positioned.fill(
            child: BackdropFilter(
              filter: ImageFilter.blur(sigmaX: 90, sigmaY: 90),
              child: Container(
                color: colors.background.withOpacity(0.80),
              ),
            ),
          ),
          
          Column(
            children: [
              // Custom Window Title Bar
              const WindowTitleBar(),

              // Main Body (Sidebar + Content)
              Expanded(
                child: Row(
                  children: [
                    // Sidebar
                    Container(
                      width: 250,
                      decoration: BoxDecoration(
                        color: colors.surface.withOpacity(0.3),
                        border: Border(right: BorderSide(color: colors.border.withOpacity(0.15))),
                      ),
                      child: ClipRRect(
                        child: BackdropFilter(
                          filter: ImageFilter.blur(sigmaX: 30, sigmaY: 30),
                          child: ListView(
                            padding: const EdgeInsets.symmetric(vertical: 20),
                            children: [
                              Padding(
                                padding: const EdgeInsets.symmetric(
                                    horizontal: 24, vertical: 12),
                                child: Row(
                                  children: [
                                    ClipRRect(
                                      borderRadius: BorderRadius.circular(8),
                                      child: Image.asset(
                                        'assets/logo.png',
                                        width: 32,
                                        height: 32,
                                        fit: BoxFit.cover,
                                      ),
                                    ),
                                    const SizedBox(width: 12),
                                    Text(
                                      'Meduza',
                                      style: TextStyle(
                                        fontSize: 24,
                                        fontWeight: FontWeight.bold,
                                        color: Theme.of(context).colorScheme.primary,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                              const SizedBox(height: 20),
                              _buildSidebarItem(
                                  Icons.home_filled, 'Home', _selectedIndex == 0, () {
                                setState(() => _selectedIndex = 0);
                              }),
                              _buildSidebarItem(
                                  Icons.search, 'Search', _selectedIndex == 1, () {
                                setState(() => _selectedIndex = 1);
                              }),
                              _buildSidebarItem(
                                  Icons.library_music, 'Library', _selectedIndex == 2,
                                  () {
                                setState(() => _selectedIndex = 2);
                              }),
                              const SizedBox(height: 20),
                              _buildSidebarItem(
                                  Icons.settings, 'Settings', _selectedIndex == 3, () {
                                setState(() => _selectedIndex = 3);
                              }),
                            ],
                          ),
                        ),
                      ),
                    ),

                    Expanded(
                      child: Padding(
                        padding: const EdgeInsets.only(bottom: 116.0),
                        child: IndexedStack(
                          index: _selectedIndex,
                          children: const [
                            DiscoverView(),
                            SearchView(),
                            LibraryView(),
                            SettingsView(),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),

          // Floating Player Bar aligned at bottom center
          const Align(
            alignment: Alignment.bottomCenter,
            child: PlayerBar(),
          ),
        ],
      ),
    );
  }

  Widget _buildSidebarItem(
      IconData icon, String title, bool isSelected, VoidCallback onTap) {
    return Container(
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      decoration: BoxDecoration(
        color: isSelected ? Colors.white.withOpacity(0.1) : Colors.transparent,
        borderRadius: BorderRadius.circular(8),
      ),
      child: ListTile(
        leading: Icon(icon, color: isSelected ? Colors.white : Colors.white54),
        title: Text(
          title,
          style: TextStyle(
            color: isSelected ? Colors.white : Colors.white54,
            fontWeight: isSelected ? FontWeight.bold : FontWeight.normal,
          ),
        ),
        onTap: onTap,
      ),
    );
  }
}

// ============================================================
// Card Progress Painter — draws progress border around the card
// ============================================================
class CardProgressPainter extends CustomPainter {
  final double progress;
  final Color borderColor;
  final double borderRadius;

  CardProgressPainter({
    required this.progress,
    required this.borderColor,
    required this.borderRadius,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final rrect = RRect.fromRectAndRadius(rect, Radius.circular(borderRadius));

    // Subtle background track
    final bgPaint = Paint()
      ..color = borderColor.withOpacity(0.08)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.0;
    canvas.drawRRect(rrect, bgPaint);

    if (progress <= 0) return;

    // Glowing active progress border
    final paint = Paint()
      ..color = borderColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5
      ..strokeCap = StrokeCap.round;

    final path = Path()..addRRect(rrect);
    final pathMetrics = path.computeMetrics();
    for (final metric in pathMetrics) {
      final extractPath = metric.extractPath(0.0, metric.length * progress);
      canvas.drawPath(extractPath, paint);
    }
  }

  @override
  bool shouldRepaint(covariant CardProgressPainter oldDelegate) {
    return oldDelegate.progress != progress ||
        oldDelegate.borderColor != borderColor ||
        oldDelegate.borderRadius != borderRadius;
  }
}

// ============================================================
// Player Bar — fully functional controls + seek bar
// ============================================================
class PlayerBar extends StatelessWidget {
  const PlayerBar({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.watch<ThemeState>().colors;

    return Consumer<PlaybackManager>(
      builder: (context, pm, child) {
        final track = pm.currentTrack;
        final isPlaying = pm.isPlaying;
        final isBuffering = pm.isBuffering;
        final isLooping = pm.isLooping;
        final isShuffling = pm.isShuffling;
        final pos = pm.position;
        final dur = pm.duration;
        final progress =
            dur.inMilliseconds > 0 ? pos.inMilliseconds / dur.inMilliseconds : 0.0;

        return SafeArea(
          child: Padding(
            padding: const EdgeInsets.only(left: 24.0, right: 24.0, bottom: 24.0),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 1000),
              child: Container(
                height: 84,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(20),
                  boxShadow: [
                    BoxShadow(
                      color: Colors.black.withOpacity(0.25),
                      blurRadius: 20,
                      offset: const Offset(0, 10),
                    ),
                  ],
                ),
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(20),
                  child: BackdropFilter(
                    filter: ImageFilter.blur(sigmaX: 30, sigmaY: 30),
                    child: Container(
                      decoration: BoxDecoration(
                        color: colors.surface.withOpacity(0.55),
                        borderRadius: BorderRadius.circular(20),
                      ),
                      child: CustomPaint(
                        painter: CardProgressPainter(
                          progress: progress,
                          borderColor: colors.accent,
                          borderRadius: 20,
                        ),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 16.0),
                          child: Row(
                            children: [
                              // LEFT: Track Info (tap to open full player)
                              Expanded(
                                flex: 3,
                                child: MouseRegion(
                                  cursor: track != null ? SystemMouseCursors.click : SystemMouseCursors.basic,
                                  child: GestureDetector(
                                    onTap: track == null ? null : () {
                                      showGeneralDialog(
                                        context: context,
                                        barrierDismissible: true,
                                        barrierLabel: 'Full Player',
                                        transitionDuration: const Duration(milliseconds: 350),
                                        transitionBuilder: (ctx, anim, secondAnim, child) {
                                          return SlideTransition(
                                            position: Tween<Offset>(
                                              begin: const Offset(0, 1),
                                              end: Offset.zero,
                                            ).animate(CurvedAnimation(parent: anim, curve: Curves.easeOutCubic)),
                                            child: child,
                                          );
                                        },
                                        pageBuilder: (ctx, _, __) => const FullScreenPlayerView(),
                                      );
                                    },
                                    child: Row(
                                      children: [
                                        Container(
                                          width: 48,
                                          height: 48,
                                          decoration: BoxDecoration(
                                            color: colors.surfaceHigh,
                                            borderRadius: BorderRadius.circular(8),
                                          ),
                                          child: track != null && track.thumbnailUrl.isNotEmpty
                                              ? ClipRRect(
                                                  borderRadius: BorderRadius.circular(8),
                                                  child: Image.network(
                                                    track.thumbnailUrl,
                                                    fit: BoxFit.cover,
                                                    errorBuilder: (context, error, stackTrace) =>
                                                        const Icon(Icons.music_note, color: Colors.white38),
                                                  ),
                                                )
                                              : const Icon(Icons.music_note, color: Colors.white38),
                                        ),
                                        const SizedBox(width: 16),
                                        Expanded(
                                          child: Column(
                                            crossAxisAlignment: CrossAxisAlignment.start,
                                            mainAxisAlignment: MainAxisAlignment.center,
                                            children: [
                                              Text(
                                                pm.error != null
                                                    ? 'Error: ${pm.error}'
                                                    : (track?.title ?? 'Nothing Playing'),
                                                style: TextStyle(
                                                  fontWeight: FontWeight.bold,
                                                  color: pm.error != null ? Colors.redAccent : colors.textPrimary,
                                                  fontSize: 14,
                                                ),
                                                maxLines: 1,
                                                overflow: TextOverflow.ellipsis,
                                              ),
                                              const SizedBox(height: 4),
                                              Text(
                                                track?.artist ?? 'Meduza',
                                                style: TextStyle(
                                                  color: colors.textSecondary,
                                                  fontSize: 12,
                                                ),
                                                maxLines: 1,
                                                overflow: TextOverflow.ellipsis,
                                              ),
                                            ],
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                              ),

                              // CENTER: Controls Row
                              Expanded(
                                flex: 4,
                                child: Row(
                                  mainAxisAlignment: MainAxisAlignment.center,
                                  children: [
                                    IconButton(
                                      icon: const Icon(Icons.shuffle, size: 20),
                                      color: isShuffling ? colors.accentGlow : colors.textSecondary.withOpacity(0.5),
                                      onPressed: () => pm.toggleShuffle(),
                                    ),
                                    const SizedBox(width: 8),
                                    IconButton(
                                      icon: const Icon(Icons.skip_previous, size: 28),
                                      color: pm.hasPrevious ? colors.textPrimary : colors.textSecondary.withOpacity(0.3),
                                      onPressed: pm.hasPrevious ? () => pm.previous() : null,
                                    ),
                                    const SizedBox(width: 8),
                                    Container(
                                      width: 44,
                                      height: 44,
                                      decoration: BoxDecoration(
                                        color: Colors.white,
                                        shape: BoxShape.circle,
                                        boxShadow: isPlaying ? [
                                          BoxShadow(
                                            color: colors.accentGlow.withOpacity(0.3),
                                            blurRadius: 12,
                                            offset: const Offset(0, 4),
                                          )
                                        ] : null,
                                      ),
                                      child: isBuffering
                                          ? const Padding(
                                              padding: EdgeInsets.all(12.0),
                                              child: CircularProgressIndicator(
                                                strokeWidth: 2,
                                                color: Colors.black,
                                              ),
                                            )
                                          : IconButton(
                                              icon: Icon(isPlaying ? Icons.pause : Icons.play_arrow, color: Colors.black, size: 24),
                                              onPressed: () => pm.playPause(),
                                            ),
                                    ),
                                    const SizedBox(width: 8),
                                    IconButton(
                                      icon: const Icon(Icons.skip_next, size: 28),
                                      color: pm.hasNext ? colors.textPrimary : colors.textSecondary.withOpacity(0.3),
                                      onPressed: pm.hasNext ? () => pm.next() : null,
                                    ),
                                    const SizedBox(width: 8),
                                    IconButton(
                                      icon: const Icon(Icons.repeat, size: 20),
                                      color: isLooping ? colors.accentGlow : colors.textSecondary.withOpacity(0.5),
                                      onPressed: () => pm.toggleLoop(),
                                    ),
                                  ],
                                ),
                              ),

                              // RIGHT: Volume Control
                              Expanded(
                                flex: 3,
                                child: Row(
                                  mainAxisAlignment: MainAxisAlignment.end,
                                  children: [
                                    IconButton(
                                      icon: Icon(
                                        pm.volume == 0
                                            ? Icons.volume_off
                                            : pm.volume < 50
                                                ? Icons.volume_down
                                                : Icons.volume_up,
                                        color: colors.textSecondary,
                                        size: 20,
                                      ),
                                      onPressed: () {
                                        if (pm.volume > 0) {
                                          pm.setVolume(0);
                                        } else {
                                          pm.setVolume(70);
                                        }
                                      },
                                    ),
                                    Container(
                                      width: 80,
                                      margin: const EdgeInsets.only(right: 16),
                                      child: SliderTheme(
                                        data: SliderThemeData(
                                          trackHeight: 4,
                                          thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 0),
                                          activeTrackColor: colors.textPrimary,
                                          inactiveTrackColor: colors.surfaceHigh,
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
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

// ============================================================
// Window Title Bar
// ============================================================
class WindowTitleBar extends StatelessWidget {
  const WindowTitleBar({super.key});

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onPanStart: (details) {
        windowManager.startDragging();
      },
      child: Container(
        height: 32,
        color: Theme.of(context).colorScheme.surface,
        child: Row(
          mainAxisAlignment: MainAxisAlignment.end,
          children: [
            IconButton(
              icon: const Icon(Icons.minimize, size: 16),
              onPressed: () => windowManager.minimize(),
              splashRadius: 16,
            ),
            IconButton(
              icon: const Icon(Icons.crop_square, size: 16),
              onPressed: () async {
                if (await windowManager.isMaximized()) {
                  windowManager.unmaximize();
                } else {
                  windowManager.maximize();
                }
              },
              splashRadius: 16,
            ),
            IconButton(
              icon: const Icon(Icons.close, size: 16),
              onPressed: () => windowManager.close(),
              splashRadius: 16,
              hoverColor: Colors.red,
            ),
          ],
        ),
      ),
    );
  }
}
