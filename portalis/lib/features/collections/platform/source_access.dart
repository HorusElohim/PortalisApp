import 'package:flutter/foundation.dart';

/// Whether generic Flutter pickers supply a stable filesystem path that Rust
/// can seed without staging another copy. Mobile plugin paths may point at
/// cache files, so they are deliberately not used there.
bool get supportsDirectPathSources =>
    defaultTargetPlatform != TargetPlatform.android &&
    defaultTargetPlatform != TargetPlatform.iOS;

/// Gallery selection stays disabled until the native readers can seed the
/// persistent platform reference directly. Enabling an `image_picker` cache
/// path here would silently put a second copy in Portalis' sandbox.
bool get supportsMobileGallerySources =>
    defaultTargetPlatform == TargetPlatform.iOS;

/// iOS and Android use their native Files pickers. iOS returns a
/// security-scoped location; Android returns a persistable SAF `content://`
/// URI that Rust reads through the native content adapter.
bool get supportsNativeFilesSources =>
    defaultTargetPlatform == TargetPlatform.iOS ||
    defaultTargetPlatform == TargetPlatform.android;

bool get supportsNoCopySources =>
    supportsDirectPathSources || supportsNativeFilesSources;

bool get supportsMediaSources =>
    supportsNoCopySources || supportsMobileGallerySources;

String get noCopySourceUnavailableMessage =>
    defaultTargetPlatform == TargetPlatform.android
        ? 'Choose files from Android Files. Gallery linking is being added '
            'without copying media into Portalis.'
        : 'Choose files from Files. Photos assets stay in Apple Photos and '
            'cannot be seeded without making a second copy.';
