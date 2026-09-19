import Foundation

/// UI bookkeeping, not a statement that a real call was blocked.
struct HarnessLoadState {
    enum Enablement: String { case checking, unknown, disabled, enabled }
    private(set) var enablement: Enablement = .checking
    private(set) var isLoading = false
    private(set) var loadedInThisSession = false

    var readyForTrial: Bool { enablement == .enabled && loadedInThisSession && !isLoading }

    mutating func setEnablement(_ value: Enablement) {
        enablement = value
        if value == .disabled || value == .unknown { loadedInThisSession = false }
    }

    mutating func beginReload() -> Bool {
        guard enablement == .enabled, !isLoading else { return false }
        isLoading = true
        loadedInThisSession = false
        return true
    }

    mutating func finishReload(success: Bool) {
        isLoading = false
        loadedInThisSession = success && enablement == .enabled
    }
}
