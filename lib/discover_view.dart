import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'theme_engine.dart';
import 'playback_manager.dart';
import 'youtube_fetcher.dart';
import 'intelligence_engine.dart';
import 'home_cache_manager.dart';
import 'playlist_manager.dart';

class DiscoverView extends StatefulWidget {
  const DiscoverView({super.key});

  @override
  State<DiscoverView> createState() => _DiscoverViewState();
}

class _DiscoverViewState extends State<DiscoverView> {
  final YouTubeFetcher _fetcher = YouTubeFetcher();

  // 5 focused, distinct genre rows — no flooding
  final List<Map<String, String>> _categories = [
    {'title': 'Trending Now',         'query': 'trending music 2024 hits'},
    {'title': 'Chill Lofi & Study',   'query': 'lofi hip hop study chill'},
    {'title': 'Late Night Vibes',     'query': 'night drive chill pop synthwave'},
    {'title': 'Workout Energy',       'query': 'gym workout hype edm'},
    {'title': 'Acoustic & Soul',      'query': 'acoustic soul guitar singer songwriter'},
  ];

  Map<String, List<TrackItem>> _musicRows = {};
  List<TrackItem> _quickPicks = [];
  bool _isInitLoading = true;
  List<TrackItem> _moodRowTracks = [];
  String _moodRowTitle = '';

  @override
  void initState() {
    super.initState();
    _loadInitialMusic();
  }

  Future<void> _loadInitialMusic() async {
    final pm = context.read<PlaybackManager>();
    final cache = await HomeCacheManager.loadCache();

    if (mounted) {
      setState(() {
        _musicRows = cache;
        _isInitLoading = false;
      });
    }

    // Build mood row based on time of day
    final hour = DateTime.now().hour;
    String moodQuery;
    if (hour < 9) {
      _moodRowTitle = 'Morning Energy';
      moodQuery = 'morning energy pop upbeat';
    } else if (hour < 13) {
      _moodRowTitle = 'Focus Flow';
      moodQuery = 'focus flow instrumental productivity';
    } else if (hour < 18) {
      _moodRowTitle = 'Afternoon Groove';
      moodQuery = 'afternoon groove funk soul r&b';
    } else if (hour < 22) {
      _moodRowTitle = 'Evening Wind Down';
      moodQuery = 'evening chill slow songs';
    } else {
      _moodRowTitle = 'Late Night';
      moodQuery = 'late night jazz ambient';
    }

    // Prefer user taste in quick picks query
    String quickPicksQuery = moodQuery;
    if (MeduzaIntelligenceEngine.artistPlayCounts.isNotEmpty) {
      final topArtist = MeduzaIntelligenceEngine.artistPlayCounts.entries
          .reduce((a, b) => a.value >= b.value ? a : b)
          .key;
      quickPicksQuery = '$topArtist mix songs';
    } else if (pm.artistPlayCounts.isNotEmpty) {
      final topArtist = pm.artistPlayCounts.entries
          .reduce((a, b) => a.value >= b.value ? a : b)
          .key;
      quickPicksQuery = '$topArtist mix songs';
    }

    // Fetch Quick Picks and Mood Row in parallel
    try {
      final qpResult = await _fetcher.searchTracks(quickPicksQuery);
      if (mounted && qpResult.isNotEmpty) {
        setState(() {
          _quickPicks = qpResult.take(8).map((v) => TrackItem(
            title: v.title,
            artist: v.author,
            mediaId: v.id.value,
            thumbnailUrl: v.thumbnails.highResUrl,
            duration: v.duration,
          )).toList();
        });
      }
    } catch (e) {
      debugPrint('Quick picks error: $e');
    }

    try {
      final moodResult = await _fetcher.searchTracks(moodQuery);
      if (mounted && moodResult.isNotEmpty) {
        setState(() {
          _moodRowTracks = moodResult.take(12).map((v) => TrackItem(
            title: v.title,
            artist: v.author,
            mediaId: v.id.value,
            thumbnailUrl: v.thumbnails.highResUrl,
            duration: v.duration,
          )).toList();
        });
      }
    } catch (e) {
      debugPrint('Mood row error: $e');
    }
  }

  void _playTrack(TrackItem track) {
    context.read<PlaybackManager>().playNow(track);
  }

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;
    final pm = context.watch<PlaybackManager>();
    final playlistManager = context.watch<PlaylistManager>();

    final likedPlaylist = playlistManager.playlists.firstWhere(
      (p) => p.id == 'liked',
      orElse: () => Playlist(id: 'liked', name: 'Liked Songs', tracks: []),
    );
    final hasFavorites = likedPlaylist.tracks.isNotEmpty;

    final hour = DateTime.now().hour;
    String greeting;
    if (hour < 12) {
      greeting = 'Good Morning';
    } else if (hour < 17) {
      greeting = 'Good Afternoon';
    } else {
      greeting = 'Good Evening';
    }

    return CustomScrollView(
      slivers: [
        // ─── Header ─────────────────────────────────────────────────────
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(32, 32, 32, 0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                ShaderMask(
                  shaderCallback: (bounds) => LinearGradient(
                    colors: [colors.textPrimary, colors.accentGlow],
                    begin: Alignment.centerLeft,
                    end: Alignment.centerRight,
                  ).createShader(bounds),
                  child: Text(
                    greeting,
                    style: const TextStyle(
                      fontSize: 36,
                      fontWeight: FontWeight.w900,
                      color: Colors.white,
                      letterSpacing: -1.5,
                    ),
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  'Curated for your taste profile',
                  style: TextStyle(
                    fontSize: 14,
                    color: colors.textSecondary.withOpacity(0.7),
                  ),
                ),
              ],
            ),
          ),
        ),

        // ─── Quick Access Cards (6-up grid) ────────────────────────────
        if ((hasFavorites || _quickPicks.isNotEmpty) && !_isInitLoading)
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(32, 28, 32, 0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Text(
                        hasFavorites ? 'Your Favorites' : 'Quick Picks',
                        style: TextStyle(
                          fontSize: 19,
                          fontWeight: FontWeight.bold,
                          color: colors.textPrimary,
                          letterSpacing: -0.3,
                        ),
                      ),
                      const SizedBox(width: 10),
                      if (hasFavorites)
                        Container(
                          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
                          decoration: BoxDecoration(
                            color: colors.accent.withOpacity(0.18),
                            borderRadius: BorderRadius.circular(20),
                          ),
                          child: Text(
                            '${likedPlaylist.tracks.length}',
                            style: TextStyle(
                              fontSize: 12,
                              color: colors.accent,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                    ],
                  ),
                  const SizedBox(height: 14),
                  _buildQuickGrid(
                    hasFavorites ? likedPlaylist.tracks.take(8).toList() : _quickPicks,
                    colors,
                  ),
                ],
              ),
            ),
          ),

        // ─── Recently Played (horizontal scroll) ───────────────────────
        if (pm.recentlyPlayed.isNotEmpty)
          SliverToBoxAdapter(
            child: _buildHorizontalSection(
              title: 'Recently Played',
              tracks: pm.recentlyPlayed,
              colors: colors,
            ),
          ),

        // ─── Time-of-Day Mood Row ───────────────────────────────────────
        if (_moodRowTracks.isNotEmpty)
          SliverToBoxAdapter(
            child: _buildHorizontalSection(
              title: _moodRowTitle,
              tracks: _moodRowTracks,
              colors: colors,
            ),
          ),

        // ─── Dynamic Category Rows ─────────────────────────────────────
        for (final cat in _categories)
          SliverToBoxAdapter(
            child: CategoryRow(
              title: cat['title']!,
              query: cat['query']!,
              musicRows: _musicRows,
              colors: colors,
              onPlay: _playTrack,
              onLoaded: (query, tracks) {
                setState(() => _musicRows[query] = tracks);
                HomeCacheManager.saveCache(_musicRows);
              },
            ),
          ),

        // Bottom padding for player bar
        const SliverToBoxAdapter(child: SizedBox(height: 120)),
      ],
    );
  }

  Widget _buildQuickGrid(List<TrackItem> tracks, MeduzaDynamicColors colors) {
    return GridView.builder(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
        maxCrossAxisExtent: 320,
        crossAxisSpacing: 12,
        mainAxisSpacing: 12,
        mainAxisExtent: 72,
      ),
      itemCount: tracks.length.clamp(0, 8),
      itemBuilder: (_, i) => _QuickPickCard(
        track: tracks[i],
        colors: colors,
        onTap: () => _playTrack(tracks[i]),
      ),
    );
  }

  Widget _buildHorizontalSection({
    required String title,
    required List<TrackItem> tracks,
    required MeduzaDynamicColors colors,
  }) {
    return Padding(
      padding: const EdgeInsets.only(top: 32, bottom: 0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 32),
            child: Text(
              title,
              style: TextStyle(
                fontSize: 19,
                fontWeight: FontWeight.bold,
                color: colors.textPrimary,
                letterSpacing: -0.3,
              ),
            ),
          ),
          const SizedBox(height: 14),
          SizedBox(
            height: 220,
            child: ListView.builder(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.only(left: 32, right: 16),
              clipBehavior: Clip.none,
              itemCount: tracks.length,
              itemBuilder: (_, i) => _MusicCard(
                track: tracks[i],
                colors: colors,
                onTap: () => _playTrack(tracks[i]),
              ),
            ),
          ),
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

// ─── CategoryRow ────────────────────────────────────────────────────────────

class CategoryRow extends StatefulWidget {
  final String title;
  final String query;
  final Map<String, List<TrackItem>> musicRows;
  final Function(String, List<TrackItem>) onLoaded;
  final Function(TrackItem) onPlay;
  final MeduzaDynamicColors colors;

  const CategoryRow({
    super.key,
    required this.title,
    required this.query,
    required this.musicRows,
    required this.onLoaded,
    required this.onPlay,
    required this.colors,
  });

  @override
  State<CategoryRow> createState() => _CategoryRowState();
}

class _CategoryRowState extends State<CategoryRow> {
  final YouTubeFetcher _fetcher = YouTubeFetcher();
  bool _isLoading = false;

  @override
  void initState() {
    super.initState();
    _loadTracks();
  }

  Future<void> _loadTracks() async {
    if (widget.musicRows.containsKey(widget.query) &&
        widget.musicRows[widget.query]!.isNotEmpty) return;

    setState(() => _isLoading = true);
    try {
      final results = await _fetcher.searchTracks(widget.query);
      if (results.isNotEmpty && mounted) {
        final tracks = results.take(20).map((v) => TrackItem(
          title: v.title,
          artist: v.author,
          mediaId: v.id.value,
          thumbnailUrl: v.thumbnails.highResUrl,
          duration: v.duration,
        )).toList();
        widget.onLoaded(widget.query, tracks);
      }
    } catch (e) {
      debugPrint('CategoryRow load error: $e');
    } finally {
      if (mounted) setState(() => _isLoading = false);
    }
  }

  @override
  void dispose() {
    _fetcher.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tracks = widget.musicRows[widget.query];

    return Padding(
      padding: const EdgeInsets.only(top: 32),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 32),
            child: Text(
              widget.title,
              style: TextStyle(
                fontSize: 19,
                fontWeight: FontWeight.bold,
                color: widget.colors.textPrimary,
                letterSpacing: -0.3,
              ),
            ),
          ),
          const SizedBox(height: 14),
          SizedBox(
            height: 220,
            child: (_isLoading || tracks == null || tracks.isEmpty)
                ? ListView.builder(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.only(left: 32),
                    itemCount: 6,
                    itemBuilder: (_, __) => _MusicSkeletonCard(colors: widget.colors),
                  )
                : ListView.builder(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.only(left: 32, right: 16),
                    clipBehavior: Clip.none,
                    itemCount: tracks.length,
                    itemBuilder: (_, i) => _MusicCard(
                      track: tracks[i],
                      colors: widget.colors,
                      onTap: () => widget.onPlay(tracks[i]),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

// ─── QuickPickCard ───────────────────────────────────────────────────────────

class _QuickPickCard extends StatefulWidget {
  final TrackItem track;
  final MeduzaDynamicColors colors;
  final VoidCallback onTap;
  const _QuickPickCard({required this.track, required this.colors, required this.onTap});
  @override
  State<_QuickPickCard> createState() => _QuickPickCardState();
}

class _QuickPickCardState extends State<_QuickPickCard> {
  bool _hovered = false;
  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: widget.onTap,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 160),
          transform: Matrix4.translationValues(0, _hovered ? -2 : 0, 0),
          decoration: BoxDecoration(
            color: _hovered
                ? widget.colors.surfaceHigh
                : widget.colors.surface.withOpacity(0.6),
            borderRadius: BorderRadius.circular(10),
            border: Border.all(
              color: _hovered
                  ? widget.colors.accent.withOpacity(0.4)
                  : Colors.white.withOpacity(0.05),
            ),
            boxShadow: _hovered
                ? [BoxShadow(
                    color: widget.colors.accentGlow.withOpacity(0.12),
                    blurRadius: 10,
                    offset: const Offset(0, 4),
                  )]
                : [],
          ),
          child: Row(
            children: [
              ClipRRect(
                borderRadius: const BorderRadius.horizontal(left: Radius.circular(9)),
                child: widget.track.thumbnailUrl.isNotEmpty
                    ? Image.network(
                        widget.track.thumbnailUrl,
                        width: 72,
                        height: double.infinity,
                        fit: BoxFit.cover,
                        errorBuilder: (_, __, ___) => Container(
                          width: 72,
                          color: widget.colors.surfaceHigh,
                          child: Icon(Icons.music_note,
                              color: widget.colors.accentGlow.withOpacity(0.5)),
                        ),
                      )
                    : Container(
                        width: 72,
                        color: widget.colors.surfaceHigh,
                        child: Icon(Icons.music_note,
                            color: widget.colors.accentGlow.withOpacity(0.5)),
                      ),
              ),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      widget.track.title,
                      style: TextStyle(
                        color: widget.colors.textPrimary,
                        fontWeight: FontWeight.w600,
                        fontSize: 13,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 3),
                    Text(
                      widget.track.artist,
                      style: TextStyle(
                        color: widget.colors.textSecondary,
                        fontSize: 11,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),
              if (_hovered)
                Padding(
                  padding: const EdgeInsets.only(right: 12),
                  child: Container(
                    width: 30,
                    height: 30,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: widget.colors.accent,
                      boxShadow: [
                        BoxShadow(
                          color: widget.colors.accentGlow.withOpacity(0.3),
                          blurRadius: 6,
                        )
                      ],
                    ),
                    child: const Icon(Icons.play_arrow, color: Colors.white, size: 18),
                  ),
                )
              else
                const SizedBox(width: 14),
            ],
          ),
        ),
      ),
    );
  }
}

// ─── MusicCard ───────────────────────────────────────────────────────────────

class _MusicCard extends StatefulWidget {
  final TrackItem track;
  final MeduzaDynamicColors colors;
  final VoidCallback onTap;
  const _MusicCard({required this.track, required this.colors, required this.onTap});
  @override
  State<_MusicCard> createState() => _MusicCardState();
}

class _MusicCardState extends State<_MusicCard> {
  bool _hovered = false;
  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: widget.onTap,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 170),
          width: 155,
          margin: const EdgeInsets.only(right: 14),
          padding: const EdgeInsets.all(11),
          transform: Matrix4.translationValues(0, _hovered ? -4 : 0, 0),
          decoration: BoxDecoration(
            color: _hovered
                ? widget.colors.surfaceHigh
                : widget.colors.surface.withOpacity(0.6),
            borderRadius: BorderRadius.circular(14),
            border: Border.all(
              color: _hovered
                  ? widget.colors.accent.withOpacity(0.35)
                  : Colors.white.withOpacity(0.05),
            ),
            boxShadow: _hovered
                ? [BoxShadow(
                    color: widget.colors.accentGlow.withOpacity(0.15),
                    blurRadius: 16,
                    offset: const Offset(0, 6),
                  )]
                : [],
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Stack(
                  children: [
                    ClipRRect(
                      borderRadius: BorderRadius.circular(10),
                      child: widget.track.thumbnailUrl.isNotEmpty
                          ? Image.network(
                              widget.track.thumbnailUrl,
                              fit: BoxFit.cover,
                              width: double.infinity,
                              height: double.infinity,
                              errorBuilder: (_, __, ___) => Container(
                                color: widget.colors.surfaceHigh,
                                child: Icon(Icons.music_note,
                                    size: 40,
                                    color: widget.colors.accentGlow.withOpacity(0.4)),
                              ),
                            )
                          : Container(
                              decoration: BoxDecoration(
                                color: widget.colors.surfaceHigh,
                                borderRadius: BorderRadius.circular(10),
                              ),
                              child: Icon(Icons.music_note,
                                  size: 40,
                                  color: widget.colors.accentGlow.withOpacity(0.4)),
                            ),
                    ),
                    if (_hovered)
                      Positioned(
                        bottom: 8,
                        right: 8,
                        child: Container(
                          width: 38,
                          height: 38,
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            color: widget.colors.accent,
                            boxShadow: [
                              BoxShadow(
                                color: Colors.black.withOpacity(0.3),
                                blurRadius: 8,
                                offset: const Offset(0, 3),
                              )
                            ],
                          ),
                          child: const Icon(Icons.play_arrow,
                              color: Colors.white, size: 22),
                        ),
                      ),
                  ],
                ),
              ),
              const SizedBox(height: 10),
              Text(
                widget.track.title,
                style: TextStyle(
                  color: widget.colors.textPrimary,
                  fontWeight: FontWeight.w600,
                  fontSize: 13,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              const SizedBox(height: 3),
              Text(
                widget.track.artist,
                style: TextStyle(
                  color: widget.colors.textSecondary,
                  fontSize: 11,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// ─── SkeletonCard ─────────────────────────────────────────────────────────────

class _MusicSkeletonCard extends StatelessWidget {
  final MeduzaDynamicColors colors;
  const _MusicSkeletonCard({required this.colors});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 155,
      margin: const EdgeInsets.only(right: 14),
      padding: const EdgeInsets.all(11),
      decoration: BoxDecoration(
        color: colors.surface.withOpacity(0.4),
        borderRadius: BorderRadius.circular(14),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: colors.surfaceHigh.withOpacity(0.4),
                borderRadius: BorderRadius.circular(10),
              ),
            ),
          ),
          const SizedBox(height: 10),
          Container(
            width: 100,
            height: 12,
            decoration: BoxDecoration(
              color: colors.surfaceHigh.withOpacity(0.4),
              borderRadius: BorderRadius.circular(4),
            ),
          ),
          const SizedBox(height: 5),
          Container(
            width: 65,
            height: 10,
            decoration: BoxDecoration(
              color: colors.surfaceHigh.withOpacity(0.25),
              borderRadius: BorderRadius.circular(4),
            ),
          ),
        ],
      ),
    );
  }
}
