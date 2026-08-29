import 'package:flutter/material.dart';

import '../design/design.dart';
import '../design/theme.dart';
import 'onboarding_controller.dart';

/// The first thing a new install shows, once — what Portalis actually does,
/// in the vocabulary it uses everywhere else (collections, peers, zero-copy)
/// rather than assuming a person already knows what those mean.
///
/// Reached exactly once per install under normal use (gated by
/// [OnboardingController]), and again on demand from Settings for anyone who
/// wants the explanation back. [onDone] is called either at the end or from
/// Skip — both count as "seen it", the same as any real onboarding flow: a
/// person who skips still shouldn't be shown it again unprompted.
class OnboardingScreen extends StatefulWidget {
  const OnboardingScreen({super.key, required this.onDone});

  final VoidCallback onDone;

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  final _controller = PageController();
  int _page = 0;

  static const _pages = [
    _OnboardingPage(
      icon: Icons.hub_outlined,
      title: 'Direct, not central',
      body: 'Portalis moves files straight between devices — yours and '
          'whoever you share with. There is no company server in the '
          'middle deciding what happens to them.',
    ),
    _OnboardingPage(
      icon: Icons.folder_special_outlined,
      title: 'Collections',
      body: 'A collection is a set of files you share or receive. Publish '
          'one and the people you choose can pull it directly from your '
          'device — no upload, no waiting on someone else\'s server.',
    ),
    _OnboardingPage(
      icon: Icons.groups_outlined,
      title: 'Peers are not the same as people',
      body: 'While a transfer is moving, you may see connections labelled '
          'by whatever the other device chose to call itself. That is a '
          'swarm peer — useful to see, but not verified. A confirmed '
          'contact is a separate, trusted relationship.',
    ),
    _OnboardingPage(
      icon: Icons.lock_open_outlined,
      title: 'Your files stay yours',
      body: 'Sharing never copies or uploads your originals anywhere — '
          'Portalis reads straight from where they already are. You '
          'decide what to share, with whom, and for how long.',
    ),
  ];

  void _next() {
    if (_page == _pages.length - 1) {
      widget.onDone();
      return;
    }
    _controller.nextPage(
      duration: const Duration(milliseconds: 320),
      curve: Curves.easeOutCubic,
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final last = _page == _pages.length - 1;
    return Scaffold(
      backgroundColor: AppColors.surfaceDeep,
      body: SafeArea(
        child: Column(
          children: [
            Align(
              alignment: Alignment.topRight,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(0, 8, 12, 0),
                child: TextButton(
                  key: const Key('onboardingSkip'),
                  onPressed: widget.onDone,
                  child: Text('Skip', style: AppText.body(color: AppColors.textDim)),
                ),
              ),
            ),
            Expanded(
              child: PageView(
                controller: _controller,
                onPageChanged: (i) => setState(() => _page = i),
                children: [for (final page in _pages) page],
              ),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 18),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  for (var i = 0; i < _pages.length; i++)
                    AnimatedContainer(
                      duration: const Duration(milliseconds: 200),
                      margin: const EdgeInsets.symmetric(horizontal: 4),
                      width: i == _page ? 22 : 7,
                      height: 7,
                      decoration: BoxDecoration(
                        color: i == _page
                            ? AppColors.signal
                            : AppColors.borderStrong,
                        borderRadius: BorderRadius.circular(AppRadius.pill),
                      ),
                    ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 0, 24, 24),
              child: ScreenAction(
                key: const Key('onboardingNext'),
                label: last ? 'Get started' : 'Next',
                onPressed: _next,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _OnboardingPage extends StatelessWidget {
  const _OnboardingPage({
    required this.icon,
    required this.title,
    required this.body,
  });

  final IconData icon;
  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(horizontal: 32),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Container(
                width: 84,
                height: 84,
                decoration: BoxDecoration(
                  color: AppColors.signalWash,
                  shape: BoxShape.circle,
                  border: Border.all(
                      color: AppColors.signal.withValues(alpha: 0.28)),
                ),
                child: Icon(icon, size: 36, color: AppColors.signal),
              ),
              const SizedBox(height: 28),
              Text(
                title,
                textAlign: TextAlign.center,
                style: displayText(size: 24, weight: FontWeight.w700),
              ),
              const SizedBox(height: 14),
              Text(
                body,
                textAlign: TextAlign.center,
                style: AppText.body(color: AppColors.textDim, height: 1.5),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
