import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'theme_engine.dart';
import 'playlist_manager.dart';
import 'playlist_view.dart';

class LibraryView extends StatefulWidget {
  const LibraryView({super.key});

  @override
  State<LibraryView> createState() => _LibraryViewState();
}

class _LibraryViewState extends State<LibraryView> {
  final TextEditingController _playlistNameController = TextEditingController();

  void _showCreatePlaylistDialog(BuildContext context, MeduzaDynamicColors colors) {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          backgroundColor: colors.surfaceHigh,
          title: Text(
            'Create Playlist',
            style: TextStyle(color: colors.textPrimary),
          ),
          content: TextField(
            controller: _playlistNameController,
            style: TextStyle(color: colors.textPrimary),
            autofocus: true,
            decoration: InputDecoration(
              hintText: 'My Playlist #1',
              hintStyle: TextStyle(color: colors.textSecondary.withOpacity(0.5)),
              enabledBorder: UnderlineInputBorder(
                borderSide: BorderSide(color: colors.border),
              ),
              focusedBorder: UnderlineInputBorder(
                borderSide: BorderSide(color: colors.accent),
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () {
                _playlistNameController.clear();
                Navigator.pop(context);
              },
              child: Text('Cancel', style: TextStyle(color: colors.textSecondary)),
            ),
            TextButton(
              onPressed: () {
                final name = _playlistNameController.text.trim();
                if (name.isNotEmpty) {
                  context.read<PlaylistManager>().createPlaylist(name);
                }
                _playlistNameController.clear();
                Navigator.pop(context);
              },
              child: Text('Create', style: TextStyle(color: colors.accent)),
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
    final playlistManager = context.watch<PlaylistManager>();
    final playlists = playlistManager.playlists;

    return Padding(
      padding: const EdgeInsets.all(32.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Library',
                    style: TextStyle(
                      fontSize: 48,
                      fontWeight: FontWeight.w800,
                      color: colors.textPrimary,
                      letterSpacing: -1.5,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Text(
                    'Your playlists and saved tracks',
                    style: TextStyle(
                      fontSize: 18,
                      color: colors.textSecondary,
                      letterSpacing: -0.5,
                    ),
                  ),
                ],
              ),
              ElevatedButton.icon(
                onPressed: () => _showCreatePlaylistDialog(context, colors),
                icon: const Icon(Icons.add),
                label: const Text('Create Playlist'),
                style: ElevatedButton.styleFrom(
                  backgroundColor: colors.accent,
                  foregroundColor: Colors.white,
                  padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 16),
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(20),
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 32),
          Expanded(
            child: playlists.isEmpty
                ? Center(
                    child: Text(
                      'No playlists yet. Create one to get started!',
                      style: TextStyle(color: colors.textSecondary),
                    ),
                  )
                : GridView.builder(
                    gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
                      crossAxisCount: 4,
                      crossAxisSpacing: 24,
                      mainAxisSpacing: 24,
                      childAspectRatio: 0.8,
                    ),
                    itemCount: playlists.length,
                    itemBuilder: (context, index) {
                      final playlist = playlists[index];
                      final trackCount = playlist.tracks.length;
                      
                      return MouseRegion(
                        cursor: SystemMouseCursors.click,
                        child: GestureDetector(
                          onTap: () {
                            Navigator.push(
                              context,
                              MaterialPageRoute(
                                builder: (context) => PlaylistView(
                                  playlistId: playlist.id,
                                  isLocal: true,
                                ),
                              ),
                            );
                          },
                          child: Container(
                            decoration: BoxDecoration(
                              color: colors.surface,
                              borderRadius: BorderRadius.circular(16),
                              border: Border.all(color: colors.border.withOpacity(0.1)),
                              boxShadow: [
                                BoxShadow(
                                  color: Colors.black.withOpacity(0.2),
                                  blurRadius: 10,
                                  offset: const Offset(0, 4),
                                )
                              ],
                            ),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Expanded(
                                  child: Container(
                                    decoration: BoxDecoration(
                                      color: colors.surfaceHigh,
                                      borderRadius: const BorderRadius.vertical(top: Radius.circular(16)),
                                    ),
                                    child: playlist.tracks.isNotEmpty && playlist.tracks.first.thumbnailUrl.isNotEmpty
                                        ? ClipRRect(
                                            borderRadius: const BorderRadius.vertical(top: Radius.circular(16)),
                                            child: Image.network(
                                              playlist.tracks.first.thumbnailUrl,
                                              width: double.infinity,
                                              height: double.infinity,
                                              fit: BoxFit.cover,
                                            ),
                                          )
                                        : Center(
                                            child: Icon(
                                              playlist.id == 'liked' ? Icons.favorite : Icons.music_note,
                                              size: 64,
                                              color: colors.accentGlow.withOpacity(0.5),
                                            ),
                                          ),
                                  ),
                                ),
                                Padding(
                                  padding: const EdgeInsets.all(16.0),
                                  child: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        playlist.name,
                                        style: TextStyle(
                                          color: colors.textPrimary,
                                          fontWeight: FontWeight.bold,
                                          fontSize: 18,
                                        ),
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                      const SizedBox(height: 4),
                                      Text(
                                        '$trackCount ${trackCount == 1 ? "track" : "tracks"}',
                                        style: TextStyle(
                                          color: colors.textSecondary,
                                          fontSize: 14,
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _playlistNameController.dispose();
    super.dispose();
  }
}
