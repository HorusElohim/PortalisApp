/// Identifies the supported text inputs without performing an action. UI may
/// use this to choose an affordance; commands remain the controller's job.
///
/// Invite codes are gone with the legacy collaboration stack. Joining a
/// collection someone shared is a Nexus contact-and-session flow rather than
/// a pasted credential, so there is no longer a third kind of text to
/// recognise here.
enum PasteKind {
  empty,
  magnet,
  search;

  static PasteKind of(String raw) {
    final value = raw.trim();
    if (value.isEmpty) return PasteKind.empty;
    if (looksLikeMagnet(value) || looksLikeInvitation(value)) {
      return PasteKind.magnet;
    }
    return PasteKind.search;
  }
}

/// A magnet URI or the bare 40-character info hash accepted by librqbit.
bool looksLikeMagnet(String value) {
  final trimmed = value.trim();
  return trimmed.startsWith('magnet:?') ||
      RegExp(r'^[0-9a-fA-F]{40}$').hasMatch(trimmed);
}

/// The `portalis://c/` invitation produced by this app's share button.
///
/// Only the shape is checked here. What the envelope actually contains — and
/// whether it is well-formed at all — is the backend's to decide, which is
/// also the only place that answer is derived from the real bytes.
bool looksLikeInvitation(String value) =>
    value.trim().startsWith(invitationPrefix);

/// Kept in step with `INVITATION_PREFIX` in the Rust protocol crate.
const invitationPrefix = 'portalis://c/';
