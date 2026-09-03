# Portalis release-build R8/ProGuard rules.
#
# minifyEnabled + shrinkResources (android/app/build.gradle) need this file
# to exist even if it starts near-empty — proguardFiles references it. Most
# of what a Flutter app needs is already covered automatically:
#
#   - Every plugin AAR (mobile_scanner, file_picker, image_picker, etc.)
#     ships its own consumer-rules.pro, which AGP merges in regardless of
#     this file's contents — no need to duplicate what mobile_scanner's own
#     ML Kit keep rules already declare.
#   - proguard-android-optimize.txt (referenced from build.gradle) already
#     keeps every `native` JNI method by class-member signature, which is
#     how flutter_rust_bridge's Rust <-> Dart calls resolve — no explicit
#     rule needed for that either.
#
# What's declared here is the one thing neither of those covers: Flutter's
# own embedding classes, kept broadly because the engine reaches them by
# name from native code that R8's call-graph analysis cannot see.
-keep class io.flutter.app.** { *; }
-keep class io.flutter.plugin.** { *; }
-keep class io.flutter.util.** { *; }
-keep class io.flutter.view.** { *; }
-keep class io.flutter.** { *; }
-keep class io.flutter.plugins.** { *; }

# flutter_rust_bridge's generated JNI glue calls into Rust by exact method
# signature. The native-methods rule above is expected to cover it via
# proguard-android-optimize.txt, but the FRB-generated Kotlin/Java class
# itself is kept explicitly too, as belt-and-braces against R8 renaming a
# field or method Rust looks up by name rather than by JNI's native-linkage
# path.
-keep class com.portalis.** { *; }

# The Flutter engine's io.flutter.embedding.engine.deferredcomponents
# package references Google Play Core's dynamic-feature-delivery classes
# (SplitInstallManager and friends) unconditionally, whether or not the app
# actually uses deferred/dynamic feature modules. Portalis does not — there
# is no play-core dependency in pubspec.yaml or build.gradle to keep those
# classes real, so R8 cannot resolve the reference and refuses to finish
# (minifyReleaseWithR8 FAILED, "Missing classes detected"). This is a known,
# widely hit Flutter/R8 interaction with no code on Portalis' side to fix;
# the standard resolution is telling R8 not to warn about a reference this
# app provably never reaches at runtime.
-dontwarn com.google.android.play.core.**
