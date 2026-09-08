import CallKit

/// Expands the user's rules into the exact-number list iOS needs.
///
/// Entries must be strictly ascending or the whole request fails. iOS often asks
/// for an incremental update; re-adding numbers it already holds fails with a
/// database constraint error, so `isIncremental` must be honoured.
///
/// Real expansion arrives with F004. This proves the extension links the core.
class CallDirectoryHandler: CXCallDirectoryProvider {
    override func beginRequest(with context: CXCallDirectoryExtensionContext) {
        _ = coreVersion()
        context.completeRequest()
    }
}
