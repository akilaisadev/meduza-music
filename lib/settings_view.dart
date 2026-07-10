import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'theme_engine.dart';

class SettingsView extends StatelessWidget {
  const SettingsView({super.key});

  @override
  Widget build(BuildContext context) {
    final themeState = context.watch<ThemeState>();
    final colors = themeState.colors;

    return Padding(
      padding: const EdgeInsets.all(32.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Settings',
            style: TextStyle(
              fontSize: 48,
              fontWeight: FontWeight.w800,
              color: colors.textPrimary,
              letterSpacing: -1.5,
            ),
          ),
          const SizedBox(height: 8),
          Text(
            'Customize your Meduza experience',
            style: TextStyle(
              fontSize: 18,
              color: colors.textSecondary,
              letterSpacing: -0.5,
            ),
          ),
          const SizedBox(height: 48),

          Expanded(
            child: ListView(
              children: [
                // Theme Hue Section
                _buildSectionHeader(colors, 'Aesthetics & Theme'),
                const SizedBox(height: 16),
                Container(
                  padding: const EdgeInsets.all(24),
                  decoration: BoxDecoration(
                    color: colors.surface,
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: colors.border.withOpacity(0.1)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Base Theme Hue',
                        style: TextStyle(
                          fontSize: 18,
                          fontWeight: FontWeight.bold,
                          color: colors.textPrimary,
                        ),
                      ),
                      const SizedBox(height: 8),
                      Text(
                        'Move the slider to shift the active mood accent of the entire interface.',
                        style: TextStyle(
                          fontSize: 14,
                          color: colors.textSecondary,
                        ),
                      ),
                      const SizedBox(height: 24),
                      Row(
                        children: [
                          Icon(Icons.palette, color: colors.accent),
                          const SizedBox(width: 16),
                          Expanded(
                            child: Slider(
                              value: themeState.hue,
                              min: 0.0,
                              max: 360.0,
                              activeColor: colors.accent,
                              inactiveColor: colors.surfaceHigh,
                              onChanged: (val) {
                                themeState.setHue(val);
                              },
                            ),
                          ),
                          const SizedBox(width: 16),
                          Text(
                            '${themeState.hue.round()}°',
                            style: TextStyle(
                              color: colors.textPrimary,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),

                const SizedBox(height: 32),

                // Recommendation Engine Info Section
                _buildSectionHeader(colors, 'Intelligence Engine Parameters'),
                const SizedBox(height: 16),
                Container(
                  padding: const EdgeInsets.all(24),
                  decoration: BoxDecoration(
                    color: colors.surface,
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: colors.border.withOpacity(0.1)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _buildEngineSignalRow(
                        colors,
                        Icons.insights,
                        'Taste Affinity',
                        'Dynamically weights your queue based on artists you listen to most frequently.',
                      ),
                      const Divider(height: 32, color: Colors.white10),
                      _buildEngineSignalRow(
                        colors,
                        Icons.timer_outlined,
                        'Recency Penalty',
                        'Applies a negative score weight to recently-played tracks to keep your feed refreshing.',
                      ),
                      const Divider(height: 32, color: Colors.white10),
                      _buildEngineSignalRow(
                        colors,
                        Icons.brightness_3_outlined,
                        'Energy Arc',
                        'Tracks the hour of the day to shift between studying, high-energy party beats, or ambient night driving vibes.',
                      ),
                    ],
                  ),
                ),

                const SizedBox(height: 32),

                // About section
                _buildSectionHeader(colors, 'About'),
                const SizedBox(height: 16),
                Container(
                  padding: const EdgeInsets.all(24),
                  decoration: BoxDecoration(
                    color: colors.surface,
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: colors.border.withOpacity(0.1)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Meduza Music v1.0.0',
                        style: TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.bold,
                          color: colors.textPrimary,
                        ),
                      ),
                      const SizedBox(height: 8),
                      Text(
                        'A high-performance music client designed for Linux desktop platforms, powered by YouTube Explode, Dart, and Flutter.',
                        style: TextStyle(
                          fontSize: 14,
                          color: colors.textSecondary,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildSectionHeader(MeduzaDynamicColors colors, String title) {
    return Text(
      title,
      style: TextStyle(
        fontSize: 20,
        fontWeight: FontWeight.bold,
        color: colors.accentGlow,
        letterSpacing: -0.5,
      ),
    );
  }

  Widget _buildEngineSignalRow(
      MeduzaDynamicColors colors, IconData icon, String title, String description) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, color: colors.accent, size: 28),
        const SizedBox(width: 16),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: TextStyle(
                  fontSize: 16,
                  fontWeight: FontWeight.bold,
                  color: colors.textPrimary,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                description,
                style: TextStyle(
                  fontSize: 13,
                  color: colors.textSecondary,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
