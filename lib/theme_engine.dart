import 'package:flutter/material.dart';

/// MEDUZA Dynamic Theme Engine
///
/// Derives a complete, harmonious color system from a single accent hue (0-360°).
/// All colors are computed using HSL color theory.
class MeduzaThemeEngine {
  static const List<MapEntry<String, double>> presetHues = [
    MapEntry("Red", 0.0),
    MapEntry("Orange", 28.0),
    MapEntry("Amber", 45.0),
    MapEntry("Yellow", 60.0),
    MapEntry("Lime", 80.0),
    MapEntry("Green", 135.0),
    MapEntry("Teal", 174.0),
    MapEntry("Sky", 200.0),
    MapEntry("Blue", 225.0),
    MapEntry("Violet", 265.0),
    MapEntry("Purple", 290.0),
    MapEntry("Pink", 330.0), // Default brand color
    MapEntry("Rose", 348.0),
  ];

  static MeduzaDynamicColors deriveColors(double hue) {
    final h = hue % 360.0;

    // Primary accent - vibrant neon
    final accent = _hsl(h, 0.85, 0.62);
    final accentDim = _hsl(h, 0.65, 0.42);
    final accentGlow = _hsl(h, 1.00, 0.72);

    // Complementary (180° opposite hue)
    final compHue = (h + 180.0) % 360.0;
    final complement = _hsl(compHue, 0.90, 0.68);

    // Triadic secondary (120° shift)
    final triadHue = (h + 120.0) % 360.0;
    final triad = _hsl(triadHue, 0.88, 0.65);

    // Dark surfaces
    final background = _hsl(h, 0.12, 0.04);
    final surface = _hsl(h, 0.09, 0.08);
    final surfaceHigh = _hsl(h, 0.07, 0.12);
    final border = _hsl(h, 0.08, 0.16);

    // Text colors
    const textPrimary = Color(0xFFEEEFF4);
    const textSecondary = Color(0xFFACAEB8);

    // Gradients
    final neonGradient = LinearGradient(
      colors: [accentGlow, complement, triad, accentGlow],
    );
    final accentGradient = LinearGradient(
      colors: [accent, accentGlow],
    );

    return MeduzaDynamicColors(
      accent: accent,
      accentDim: accentDim,
      accentGlow: accentGlow,
      complement: complement,
      triad: triad,
      background: background,
      surface: surface,
      surfaceHigh: surfaceHigh,
      border: border,
      textPrimary: textPrimary,
      textSecondary: textSecondary,
      neonGradient: neonGradient,
      accentGradient: accentGradient,
      hue: h,
    );
  }

  static Color _hsl(double h, double s, double l) {
    return HSLColor.fromAHSL(1.0, h % 360.0, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)).toColor();
  }
}

class MeduzaDynamicColors {
  final Color accent;
  final Color accentDim;
  final Color accentGlow;
  final Color complement;
  final Color triad;
  final Color background;
  final Color surface;
  final Color surfaceHigh;
  final Color border;
  final Color textPrimary;
  final Color textSecondary;
  final LinearGradient neonGradient;
  final LinearGradient accentGradient;
  final double hue;

  MeduzaDynamicColors({
    required this.accent,
    required this.accentDim,
    required this.accentGlow,
    required this.complement,
    required this.triad,
    required this.background,
    required this.surface,
    required this.surfaceHigh,
    required this.border,
    required this.textPrimary,
    required this.textSecondary,
    required this.neonGradient,
    required this.accentGradient,
    required this.hue,
  });
}

/// ThemeState — ChangeNotifier so it can be watched from any widget tree.
class ThemeState extends ChangeNotifier {
  double _hue = 330.0; // Default Pink
  late MeduzaDynamicColors colors;

  ThemeState() {
    colors = MeduzaThemeEngine.deriveColors(_hue);
  }

  double get hue => _hue;

  void setHue(double newHue) {
    _hue = newHue;
    colors = MeduzaThemeEngine.deriveColors(_hue);
    notifyListeners();
  }
}
