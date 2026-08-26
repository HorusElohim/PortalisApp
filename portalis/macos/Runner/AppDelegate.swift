import Cocoa
import FlutterMacOS
import ImageIO
import UniformTypeIdentifiers

@main
class AppDelegate: FlutterAppDelegate {
  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}

/// Keeps the sandbox permission that a file selection granted, across restarts.
///
/// A sandboxed Mac app may read a file the person chose, but only for as long
/// as the process that asked lives. Reopening Portalis therefore lost access to
/// every source it was still seeding, and Rust — which reads those originals
/// directly, because sharing never copies media — got `Operation not permitted`
/// the moment it tried to hash one again.
///
/// The fix is the same one iOS already uses: keep a security-scoped bookmark
/// for each selected file and resolve it at launch. This is a permission
/// record, not a copy — no media bytes are stored here, only the sandbox's own
/// handle to a location that stays exactly where the person put it.
final class SecurityScopedSources {
  private static let channelName = "app.portalis/security-scoped-sources"
  private static let bookmarksKey = "portalis.security-scoped-source-bookmarks"

  /// Held for the lifetime of the process. Dropping a URL ends the access it
  /// represents, so these stay retained rather than being balanced with a
  /// matching stop: the access is wanted for exactly as long as Portalis runs.
  private var activeURLs: [String: URL] = [:]

  func register(with messenger: FlutterBinaryMessenger) {
    restoreAccess()
    let channel = FlutterMethodChannel(name: Self.channelName, binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterMethodNotImplemented)
        return
      }
      switch call.method {
      case "retain":
        guard let arguments = call.arguments as? [String: Any],
              let paths = arguments["paths"] as? [String] else {
          result(FlutterError(
            code: "invalid_arguments",
            message: "retain needs a list of paths.",
            details: nil
          ))
          return
        }
        result(self.retain(paths))
      case "release":
        guard let arguments = call.arguments as? [String: Any],
              let paths = arguments["paths"] as? [String] else {
          result(FlutterError(
            code: "invalid_arguments",
            message: "release needs a list of paths.",
            details: nil
          ))
          return
        }
        self.release(paths)
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  /// Records the sandbox's permission for each freshly chosen file, and
  /// answers with the ones that are genuinely readable afterwards.
  ///
  /// Called immediately after a pick, while the app still holds the implicit
  /// access that selection granted — a bookmark cannot be created later.
  private func retain(_ paths: [String]) -> [String] {
    var retained: [String] = []
    for path in paths {
      let url = URL(fileURLWithPath: path)
      guard FileManager.default.isReadableFile(atPath: path) else { continue }
      do {
        let data = try url.bookmarkData(
          options: .withSecurityScope,
          includingResourceValuesForKeys: nil,
          relativeTo: nil
        )
        var bookmarks = savedBookmarks
        bookmarks[path] = data
        UserDefaults.standard.set(bookmarks, forKey: Self.bookmarksKey)
        // Start access now too, so the very first publish reads through the
        // same scoped URL a later launch will resolve.
        if activeURLs[path] == nil, url.startAccessingSecurityScopedResource() {
          activeURLs[path] = url
        }
        retained.append(path)
      } catch {
        // An unbookmarkable location is reported rather than hidden: the
        // caller decides whether to warn, and a silent success here would
        // become a failure to seed on the next launch instead.
        NSLog("Portalis could not retain access to \(path): \(error.localizedDescription)")
      }
    }
    return retained
  }

  /// Forgets files Portalis no longer seeds, so a deleted collection does not
  /// keep a permission — or a stale bookmark — alive forever.
  private func release(_ paths: [String]) {
    var bookmarks = savedBookmarks
    for path in paths {
      if let url = activeURLs.removeValue(forKey: path) {
        url.stopAccessingSecurityScopedResource()
      }
      bookmarks.removeValue(forKey: path)
    }
    UserDefaults.standard.set(bookmarks, forKey: Self.bookmarksKey)
  }

  private var savedBookmarks: [String: Data] {
    guard let values = UserDefaults.standard.dictionary(forKey: Self.bookmarksKey) else {
      return [:]
    }
    return values.reduce(into: [:]) { bookmarks, entry in
      if let data = entry.value as? Data {
        bookmarks[entry.key] = data
      }
    }
  }

  /// Resolves every retained bookmark before Flutter starts the backend, so
  /// the first thing Rust does on a restart — rehydrating what it was seeding
  /// — happens with the same access the original selection had.
  private func restoreAccess() {
    var bookmarks = savedBookmarks
    var changed = false
    for (path, bookmark) in bookmarks {
      var stale = false
      guard let url = try? URL(
        resolvingBookmarkData: bookmark,
        options: .withSecurityScope,
        relativeTo: nil,
        bookmarkDataIsStale: &stale
      ), url.startAccessingSecurityScopedResource() else {
        // The file is gone, or the permission no longer resolves. Dropping the
        // record keeps a moved source from being retried on every launch; the
        // collection itself survives and reports the source as missing.
        bookmarks.removeValue(forKey: path)
        changed = true
        continue
      }
      activeURLs[path] = url
      if stale, let refreshed = try? url.bookmarkData(
        options: .withSecurityScope,
        includingResourceValuesForKeys: nil,
        relativeTo: nil
      ) {
        bookmarks[path] = refreshed
        changed = true
      }
    }
    if changed {
      UserDefaults.standard.set(bookmarks, forKey: Self.bookmarksKey)
    }
  }
}

final class HeicPreview {
  private static let channelName = "app.portalis/heic-preview"

  func register(with messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(name: Self.channelName, binaryMessenger: messenger)
    channel.setMethodCallHandler { call, result in
      guard call.method == "decode",
            let arguments = call.arguments as? [String: Any],
            let path = arguments["path"] as? String else {
        result(FlutterMethodNotImplemented)
        return
      }
      let maxPixelSize = arguments["maxPixelSize"] as? Int ?? 1024
      DispatchQueue.global(qos: .userInitiated).async {
        let data = Self.decode(path: path, maxPixelSize: maxPixelSize)
        DispatchQueue.main.async {
          guard let data else {
            result(FlutterError(
              code: "decode_failed",
              message: "The platform could not decode this HEIC image.",
              details: nil
            ))
            return
          }
          result(FlutterStandardTypedData(bytes: data))
        }
      }
    }
  }

  private static func decode(path: String, maxPixelSize: Int) -> Data? {
    guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil) else {
      return nil
    }
    let options: [CFString: Any] = [
      kCGImageSourceCreateThumbnailFromImageAlways: true,
      kCGImageSourceCreateThumbnailWithTransform: true,
      kCGImageSourceThumbnailMaxPixelSize: max(64, min(maxPixelSize, 2048)),
    ]
    guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else {
      return nil
    }
    let output = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(
      output,
      UTType.jpeg.identifier as CFString,
      1,
      nil
    ) else {
      return nil
    }
    CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.9] as CFDictionary)
    guard CGImageDestinationFinalize(destination) else { return nil }
    return output as Data
  }
}
