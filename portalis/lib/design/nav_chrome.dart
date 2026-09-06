// Cross-feature navigation-chrome design primitive.
//
// One idea of what the app's navigation chrome *is*, shared by the mobile
// bottom bar and the desktop top bar rather than each inventing its own
// container and selected-state treatment. The bar relocates and reshapes
// itself across the responsive breakpoint; the glass, the glow and the
// motion it's made of do not.

import 'dart:ui';

import 'package:flutter/material.dart';

import 'theme.dart';

/// Floating glass shell for the app's navigation chrome.
///
/// [intensity] ties its border glow to real throughput (see
/// [Glow.intensityForRate]) the same way every other energised surface in
/// the app does — chrome that quietly brightens while something is actually
/// moving, and settles back to a calm outline the instant it isn't. Never
/// decorative on its own: the glow still means *data is moving*, wherever
/// this dock is pinned.
class NavDock extends StatelessWidget {
  const NavDock({
    super.key,
    required this.child,
    this.intensity = 0,
    this.radius = AppRadius.card,
  });

  final Widget child;

  /// 0 = nothing moving, 1 = saturated.
  final double intensity;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final glow = Glow.of(
      intensity > 0 ? GlowLevel.active : GlowLevel.calm,
      intensity: intensity,
    );
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 22, sigmaY: 22),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 420),
          curve: Curves.easeOutCubic,
          decoration: BoxDecoration(
            color: AppColors.surfaceSunken.withValues(alpha: 0.64),
            borderRadius: BorderRadius.circular(radius),
            border: glow.border,
            boxShadow: glow.shadows,
          ),
          child: child,
        ),
      ),
    );
  }
}

/// The one animated "selected" treatment every destination in the nav
/// chrome earns: a signal-tinted wash and ring that fades in and out, rather
/// than each call site choosing its own colour switch. Used by the mobile
/// bottom bar's tabs and the desktop top bar's actions alike, so a person
/// moving between window sizes is looking at the same control, not two
/// designs that happen to sit in similar places.
class NavSelection extends StatelessWidget {
  const NavSelection({
    super.key,
    required this.selected,
    required this.child,
    this.padding = const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
    this.radius = AppRadius.inner,
  });

  final bool selected;
  final Widget child;
  final EdgeInsets padding;
  final double radius;

  @override
  Widget build(BuildContext context) => AnimatedContainer(
        duration: const Duration(milliseconds: 260),
        curve: Curves.easeOutCubic,
        padding: padding,
        decoration: BoxDecoration(
          color: selected
              ? AppColors.signal.withValues(alpha: 0.14)
              : Colors.transparent,
          borderRadius: BorderRadius.circular(radius),
          border: Border.all(
            color: selected
                ? AppColors.signal.withValues(alpha: 0.35)
                : Colors.transparent,
          ),
        ),
        child: child,
      );
}
