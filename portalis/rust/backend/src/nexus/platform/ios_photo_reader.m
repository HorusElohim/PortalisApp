#import <Foundation/Foundation.h>
#import <AVFoundation/AVFoundation.h>
#import <Photos/Photos.h>

#include <fcntl.h>
#include <stdio.h>
#include <sys/stat.h>
#include <unistd.h>

// Every PhotoKit callback here used DISPATCH_TIME_FOREVER, so a stalled
// iCloud fetch or a revoked permission prompt could block a Rust worker
// thread permanently. Bounded instead: each wait has an explicit budget, and
// a timed-out wait returns a distinct, actionable error rather than hanging.
static const int64_t kPortalisPhotoKitPermissionTimeoutSeconds = 30;
static const int64_t kPortalisPhotoKitMetadataTimeoutSeconds = 20;
static const int64_t kPortalisPhotoKitReadTimeoutSeconds = 60;

// Waits up to `seconds`. Returns YES if the semaphore signalled, NO on
// timeout — the caller decides what a timeout means for its own request.
static BOOL PortalisWaitBounded(dispatch_semaphore_t semaphore, int64_t seconds) {
  dispatch_time_t deadline = dispatch_time(DISPATCH_TIME_NOW, seconds * NSEC_PER_SEC);
  return dispatch_semaphore_wait(semaphore, deadline) == 0;
}

int portalis_photo_asset_import(
    const char *path,
    bool video,
    char *identifier,
    size_t identifier_capacity) {
  @autoreleasepool {
    if (path == NULL || identifier == NULL || identifier_capacity == 0) return -1;
    identifier[0] = '\0';
    NSString *sourcePath = [NSString stringWithUTF8String:path];
    if (sourcePath == nil || ![[NSFileManager defaultManager] fileExistsAtPath:sourcePath]) return -2;
    NSURL *sourceURL = [NSURL fileURLWithPath:sourcePath];

    __block PHAuthorizationStatus authorization =
        [PHPhotoLibrary authorizationStatusForAccessLevel:PHAccessLevelAddOnly];
    if (authorization == PHAuthorizationStatusNotDetermined) {
      dispatch_semaphore_t permission = dispatch_semaphore_create(0);
      [PHPhotoLibrary requestAuthorizationForAccessLevel:PHAccessLevelAddOnly
        handler:^(PHAuthorizationStatus status) {
          authorization = status;
          dispatch_semaphore_signal(permission);
        }];
      if (!PortalisWaitBounded(permission, kPortalisPhotoKitPermissionTimeoutSeconds)) {
        return -6; // The permission prompt never resolved within the budget.
      }
    }
    if (authorization != PHAuthorizationStatusAuthorized) return -3;

    __block NSString *assetIdentifier = nil;
    __block BOOL success = NO;
    dispatch_semaphore_t completed = dispatch_semaphore_create(0);
    [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
      PHAssetCreationRequest *request = [PHAssetCreationRequest creationRequestForAsset];
      PHAssetResourceCreationOptions *options = [[PHAssetResourceCreationOptions alloc] init];
      options.originalFilename = sourceURL.lastPathComponent;
      // Copy first. Rust persists and rebinds the returned asset before the
      // sandbox source is removed, which makes the move recoverable.
      options.shouldMoveFile = NO;
      [request addResourceWithType:(video ? PHAssetResourceTypeVideo : PHAssetResourceTypePhoto)
                            fileURL:sourceURL
                            options:options];
      assetIdentifier = request.placeholderForCreatedAsset.localIdentifier;
    } completionHandler:^(BOOL didSucceed, NSError *error) {
      if (!didSucceed && error != nil) {
        fprintf(stderr, "Portalis could not import media into Photos: %s\\n", error.localizedDescription.UTF8String);
      }
      success = didSucceed;
      dispatch_semaphore_signal(completed);
    }];
    if (!PortalisWaitBounded(completed, kPortalisPhotoKitReadTimeoutSeconds)) {
      return -7; // The library write never completed within the budget.
    }
    if (!success || assetIdentifier.length == 0) return -4;
    const char *utf8 = assetIdentifier.UTF8String;
    size_t length = strlen(utf8);
    if (length + 1 > identifier_capacity) return -5;
    memcpy(identifier, utf8, length + 1);
    return 0;
  }
}

static PHAssetResource *PortalisPrimaryResource(PHAsset *asset) {
  NSArray<PHAssetResource *> *resources = [PHAssetResource assetResourcesForAsset:asset];
  NSArray<NSNumber *> *preferred = asset.mediaType == PHAssetMediaTypeVideo
      ? @[@(PHAssetResourceTypeVideo), @(PHAssetResourceTypeFullSizeVideo)]
      : @[@(PHAssetResourceTypeFullSizePhoto), @(PHAssetResourceTypePhoto)];
  for (NSNumber *type in preferred) {
    for (PHAssetResource *resource in resources) {
      if (resource.type == type.integerValue) return resource;
    }
  }
  return nil;
}

static NSURL *PortalisDirectAssetURL(PHAsset *asset, NSError **outError) {
  static NSMutableDictionary<NSString *, NSURL *> *urls;
  static dispatch_once_t once;
  dispatch_once(&once, ^{ urls = [NSMutableDictionary dictionary]; });
  @synchronized (urls) {
    NSURL *cached = urls[asset.localIdentifier];
    if (cached != nil) return cached;
  }

  __block NSURL *url = nil;
  __block NSError *requestError = nil;
  dispatch_semaphore_t done = dispatch_semaphore_create(0);
  if (asset.mediaType == PHAssetMediaTypeVideo) {
    PHVideoRequestOptions *options = [[PHVideoRequestOptions alloc] init];
    options.networkAccessAllowed = YES;
    options.version = PHVideoRequestOptionsVersionOriginal;
    [[PHImageManager defaultManager]
      requestAVAssetForVideo:asset
      options:options
      resultHandler:^(AVAsset *avAsset, AVAudioMix *audioMix, NSDictionary *info) {
        (void)audioMix;
        if ([avAsset isKindOfClass:AVURLAsset.class]) {
          url = ((AVURLAsset *)avAsset).URL;
        }
        requestError = info[PHImageErrorKey];
        dispatch_semaphore_signal(done);
      }];
  } else {
    PHContentEditingInputRequestOptions *options = [[PHContentEditingInputRequestOptions alloc] init];
    options.networkAccessAllowed = YES;
    [asset requestContentEditingInputWithOptions:options completionHandler:^(PHContentEditingInput *input, NSDictionary *info) {
      url = input.fullSizeImageURL;
      requestError = info[PHContentEditingInputErrorKey];
      dispatch_semaphore_signal(done);
    }];
  }
  if (!PortalisWaitBounded(done, kPortalisPhotoKitMetadataTimeoutSeconds)) {
    // No direct URL within budget; the caller falls back to the sequential
    // reader rather than blocking forever on a slow iCloud fetch.
    fprintf(stderr, "Portalis direct Photos URL request for %s timed out, falling back to the byte stream\\n", asset.localIdentifier.UTF8String);
    return nil;
  }
  if (url != nil) {
    @synchronized (urls) { urls[asset.localIdentifier] = url; }
    // Logged once per asset (immediately after caching, so a second call
    // for the same asset hits the early cache return above and never
    // re-logs) — this is the "did we get a real direct file URL, or are we
    // about to fall back to the slow PhotoKit byte stream" boundary.
    fprintf(stderr, "Portalis resolved direct Photos URL for %s\\n", asset.localIdentifier.UTF8String);
  }
  if (outError != NULL) *outError = requestError;
  return url;
}

static int PortalisReadDirectURL(NSURL *url, uint64_t offset, uint8_t *buffer, size_t length) {
  int descriptor = open(url.fileSystemRepresentation, O_RDONLY);
  if (descriptor < 0) return -1;
  size_t copied = 0;
  while (copied < length) {
    ssize_t count = pread(descriptor, buffer + copied, length - copied, (off_t)(offset + copied));
    if (count <= 0) break;
    copied += (size_t)count;
  }
  close(descriptor);
  return copied == length ? 0 : -1;
}

// A sequential, single-pass reader over one asset's PhotoKit resource
// stream, for assets with no direct file URL (typically iCloud-backed).
//
// The old implementation re-issued `requestDataForAssetResource:` from byte
// zero for every single read call — an O(n^2) restart, worst on exactly the
// large/iCloud assets ADR-0014 exists for — and waited on a semaphore that
// could never expire. This type keeps one PhotoKit request alive per asset,
// buffers only what has not yet been consumed, and is built for the
// consecutive/forward access pattern torrent piece hashing actually makes:
// out-of-order or backward reads are refused rather than silently
// re-fetching (which would reintroduce the same quadratic cost), and a
// caller that needs random access should route through the direct-URL path.
@interface PortalisPhotoSequentialReader : NSObject
@property(nonatomic, strong) NSMutableData *buffer;
@property(nonatomic, assign) uint64_t consumedOffset;   // Bytes already returned to callers.
@property(nonatomic, assign) uint64_t bufferedOffset;   // Absolute stream offset `buffer` starts at.
@property(nonatomic, assign) BOOL finished;
@property(nonatomic, strong) NSError *error;
@property(nonatomic, assign) PHAssetResourceDataRequestID requestID;
@property(nonatomic, strong) dispatch_semaphore_t dataAvailable;
@property(nonatomic, strong) NSLock *lock;
@end

@implementation PortalisPhotoSequentialReader
@end

static NSMutableDictionary<NSString *, PortalisPhotoSequentialReader *> *PortalisSequentialReaders(void) {
  static NSMutableDictionary<NSString *, PortalisPhotoSequentialReader *> *readers;
  static dispatch_once_t once;
  dispatch_once(&once, ^{ readers = [NSMutableDictionary dictionary]; });
  return readers;
}

// Starts (or reuses) the one live sequential request for `identifier`.
// Cancelled and forgotten once every byte up to `targetEnd` has been
// buffered, so a completed range never keeps PhotoKit streaming further
// bytes nobody asked for.
static PortalisPhotoSequentialReader *PortalisSequentialReaderStart(
    NSString *identifier, PHAssetResource *resource) {
  NSMutableDictionary *readers = PortalisSequentialReaders();
  @synchronized (readers) {
    PortalisPhotoSequentialReader *existing = readers[identifier];
    if (existing != nil && !existing.finished && existing.error == nil) return existing;

    PortalisPhotoSequentialReader *reader = [[PortalisPhotoSequentialReader alloc] init];
    reader.buffer = [NSMutableData data];
    reader.consumedOffset = 0;
    reader.bufferedOffset = 0;
    reader.finished = NO;
    reader.dataAvailable = dispatch_semaphore_create(0);
    reader.lock = [[NSLock alloc] init];
    readers[identifier] = reader;

    // Logged once per stream (right here, not per read call) — the moment
    // Portalis actually starts pulling bytes through PhotoKit's slower
    // resource-manager stream instead of a direct file handle.
    fprintf(stderr, "Portalis opened the Photos byte stream for %s\\n", identifier.UTF8String);

    PHAssetResourceRequestOptions *options = [[PHAssetResourceRequestOptions alloc] init];
    options.networkAccessAllowed = YES;
    __weak PortalisPhotoSequentialReader *weakReader = reader;
    PHAssetResourceDataRequestID requestID = [[PHAssetResourceManager defaultManager]
      requestDataForAssetResource:resource
      options:options
      dataReceivedHandler:^(NSData *data) {
        PortalisPhotoSequentialReader *strongReader = weakReader;
        if (strongReader == nil) return;
        [strongReader.lock lock];
        [strongReader.buffer appendData:data];
        [strongReader.lock unlock];
        dispatch_semaphore_signal(strongReader.dataAvailable);
      }
      completionHandler:^(NSError *error) {
        PortalisPhotoSequentialReader *strongReader = weakReader;
        if (strongReader == nil) return;
        [strongReader.lock lock];
        strongReader.finished = YES;
        strongReader.error = error;
        [strongReader.lock unlock];
        dispatch_semaphore_signal(strongReader.dataAvailable);
      }];
    reader.requestID = requestID;
    return reader;
  }
}

static void PortalisSequentialReaderCancelAndForget(NSString *identifier, PortalisPhotoSequentialReader *reader) {
  NSMutableDictionary *readers = PortalisSequentialReaders();
  @synchronized (readers) {
    if (readers[identifier] == reader) {
      [readers removeObjectForKey:identifier];
    }
  }
  if (reader.requestID != 0) {
    [[PHAssetResourceManager defaultManager] cancelDataRequest:reader.requestID];
  }
}

// Reads `[offset, offset+length)` sequentially. `offset` must equal the
// reader's already-consumed position (this asset's next unread byte);
// anything else is refused rather than restarted from zero, matching the
// forward-only contract the torrent hasher and piece reader actually use.
static int PortalisReadSequential(
    NSString *identifier, PHAssetResource *resource,
    uint64_t offset, uint8_t *buffer, size_t length) {
  PortalisPhotoSequentialReader *reader = PortalisSequentialReaderStart(identifier, resource);

  [reader.lock lock];
  BOOL positionMismatch = offset != reader.consumedOffset;
  [reader.lock unlock];
  if (positionMismatch) {
    // Out-of-order access on the streaming-only path. The caller (Rust) is
    // expected to serialize reads per asset; refusing here surfaces that
    // programming error instead of silently paying to re-fetch from zero.
    return -4;
  }

  dispatch_time_t deadline = dispatch_time(
      DISPATCH_TIME_NOW, kPortalisPhotoKitReadTimeoutSeconds * NSEC_PER_SEC);
  size_t copied = 0;
  while (copied < length) {
    [reader.lock lock];
    uint64_t haveUpTo = reader.bufferedOffset + reader.buffer.length;
    uint64_t wantUpTo = offset + copied;
    BOOL finished = reader.finished;
    NSError *error = reader.error;
    if (haveUpTo > wantUpTo) {
      NSUInteger start = (NSUInteger)(wantUpTo - reader.bufferedOffset);
      NSUInteger available = (NSUInteger)(haveUpTo - wantUpTo);
      NSUInteger want = length - copied;
      NSUInteger take = MIN(available, want);
      memcpy(buffer + copied, ((const uint8_t *)reader.buffer.bytes) + start, take);
      copied += take;
      // Drop consumed bytes so the buffer never grows past what one
      // outstanding request window needs.
      if (start + take == reader.buffer.length) {
        reader.bufferedOffset = haveUpTo;
        [reader.buffer setLength:0];
      }
      [reader.lock unlock];
      continue;
    }
    [reader.lock unlock];
    if (finished) {
      // Stream ended before satisfying the request: short read or error.
      if (error != nil) {
        fprintf(stderr, "Portalis sequential Photos read for %s failed: %s\\n",
                identifier.UTF8String, error.localizedDescription.UTF8String);
      }
      break;
    }
    long waited = dispatch_semaphore_wait(reader.dataAvailable, deadline);
    if (waited != 0) {
      // Timed out waiting for more bytes.
      break;
    }
  }

  [reader.lock lock];
  reader.consumedOffset = offset + copied;
  BOOL doneStreaming = reader.finished;
  [reader.lock unlock];

  if (copied == length) {
    // If this read reached (or passed) end of stream, the request has
    // nothing left to deliver — cancel and forget it now rather than
    // leaving a finished-but-cached entry that the next asset access would
    // otherwise have to notice and skip. A completed read that has *not*
    // reached the end deliberately keeps the request alive for the caller's
    // next consecutive read.
    if (doneStreaming) {
      PortalisSequentialReaderCancelAndForget(identifier, reader);
    }
    return 0;
  }
  // Short read: cancel this request so a caller that retries starts clean
  // rather than resuming a broken stream.
  PortalisSequentialReaderCancelAndForget(identifier, reader);
  return -3;
}

bool portalis_photo_asset_available(const char *identifier) {
  @autoreleasepool {
    if (identifier == NULL) return false;
    NSString *assetId = [NSString stringWithUTF8String:identifier];
    PHFetchResult<PHAsset *> *assets = [PHAsset fetchAssetsWithLocalIdentifiers:@[assetId] options:nil];
    return assets.count == 1 && PortalisPrimaryResource(assets.firstObject) != nil;
  }
}

int64_t portalis_photo_asset_length(const char *identifier) {
  @autoreleasepool {
    if (identifier == NULL) return -1;
    NSString *assetId = [NSString stringWithUTF8String:identifier];
    PHFetchResult<PHAsset *> *assets = [PHAsset fetchAssetsWithLocalIdentifiers:@[assetId] options:nil];
    PHAsset *asset = assets.count == 1 ? assets.firstObject : nil;
    PHAssetResource *resource = asset == nil ? nil : PortalisPrimaryResource(asset);
    if (resource == nil) return -2;

    NSError *directError = nil;
    NSURL *url = PortalisDirectAssetURL(asset, &directError);
    struct stat attributes;
    if (url != nil && stat(url.fileSystemRepresentation, &attributes) == 0 && attributes.st_size > 0) {
      return attributes.st_size;
    }
    if (directError != nil) {
      fprintf(stderr, "Portalis could not access direct Photos URL for %s: %s\\n", identifier, directError.localizedDescription.UTF8String);
    }

    __block uint64_t received = 0;
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    PHAssetResourceRequestOptions *options = [[PHAssetResourceRequestOptions alloc] init];
    options.networkAccessAllowed = YES;
    PHAssetResourceDataRequestID requestID = [[PHAssetResourceManager defaultManager]
      requestDataForAssetResource:resource
      options:options
      dataReceivedHandler:^(NSData *data) { received += data.length; }
      completionHandler:^(NSError *error) {
        if (error != nil) {
          fprintf(stderr, "Portalis PhotoKit length request for %s completed with error: %s\\n", identifier, error.localizedDescription.UTF8String);
        }
        dispatch_semaphore_signal(done);
      }];
    if (!PortalisWaitBounded(done, kPortalisPhotoKitReadTimeoutSeconds)) {
      [[PHAssetResourceManager defaultManager] cancelDataRequest:requestID];
      return -4; // Length probe timed out; caller should not treat 0/negative as "empty".
    }
    return received > 0 && received <= INT64_MAX ? (int64_t)received : -3;
  }
}

// Reads an exact range without materialising the original asset in Portalis'
// container. Direct-URL assets get exact, zero-copy pread(2) access; assets
// with no direct URL are served by the bounded sequential reader above,
// which streams each byte of the resource at most once rather than
// restarting PhotoKit's delivery for every call.
int portalis_photo_asset_read(
    const char *identifier,
    uint64_t offset,
    uint8_t *buffer,
    size_t length) {
  @autoreleasepool {
    if (identifier == NULL || buffer == NULL) return -1;
    NSString *assetId = [NSString stringWithUTF8String:identifier];
    PHFetchResult<PHAsset *> *assets = [PHAsset fetchAssetsWithLocalIdentifiers:@[assetId] options:nil];
    PHAsset *asset = assets.count == 1 ? assets.firstObject : nil;
    PHAssetResource *resource = asset == nil ? nil : PortalisPrimaryResource(asset);
    if (resource == nil) return -2;

    NSError *directError = nil;
    NSURL *url = PortalisDirectAssetURL(asset, &directError);
    if (url != nil && PortalisReadDirectURL(url, offset, buffer, length) == 0) {
      return 0;
    }
    if (directError != nil) {
      fprintf(stderr, "Portalis could not access direct Photos URL for %s: %s\\n", identifier, directError.localizedDescription.UTF8String);
    }

    return PortalisReadSequential(assetId, resource, offset, buffer, length);
  }
}
