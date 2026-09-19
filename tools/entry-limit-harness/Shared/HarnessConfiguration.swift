import Foundation

/// Fixture decoding and OS-entry transport only. This is not number-plan validation
/// or a filtering engine: exact fixtures must contain an already validated number.
struct HarnessConfiguration {
    enum Mode: String, Decodable { case empty, exact, blocking, identification }

    private struct Payload: Decodable {
        let version: Int
        let alias: String
        let mode: Mode
        let number: String?
        let count: Int?
        let syntheticBase: String?
        let progressEvery: Int
        let retryDelaySeconds: Double
        let maximumRetries: Int
    }
    // Only decode() can construct a validated configuration.
    private let payload: Payload
    var alias: String { payload.alias }
    var mode: Mode { payload.mode }
    var retryDelaySeconds: Double { payload.retryDelaySeconds }
    var maximumRetries: Int { payload.maximumRetries }
    private var number: String? { payload.number }
    private var count: Int? { payload.count }
    private var syntheticBase: String? { payload.syntheticBase }
    private var progressEvery: Int { payload.progressEvery }

    var entryCount: Int {
        switch mode {
        case .empty: return 0
        case .exact: return 1
        case .blocking, .identification: return count ?? 0
        }
    }

    var isCapacityTest: Bool { mode == .blocking || mode == .identification }

    enum FixtureError: Error, LocalizedError {
        case missing, malformed, invalidVersion, invalidAlias, invalidNumber
        case invalidCount, invalidControls, unexpectedField, overflow

        var errorDescription: String? {
            switch self {
            case .missing: return "Bundled fixture is missing. Rebuild with the probe script."
            case .malformed: return "Fixture JSON is malformed or has unsupported fields."
            case .invalidVersion: return "Unsupported fixture version."
            case .invalidAlias: return "Fixture alias must use letters, digits, hyphens or underscores."
            case .invalidNumber: return "Expected one canonical international number. No number was loaded."
            case .invalidCount: return "Capacity count must be a nonnegative integer."
            case .invalidControls: return "Invalid progress or retry configuration."
            case .unexpectedField: return "Fixture fields do not match its mode."
            case .overflow: return "Entries exceed the Call Directory integer range."
            }
        }
    }

    static func decode(_ data: Data) throws -> Self {
        let allowed = Set(["version", "alias", "mode", "number", "count", "syntheticBase",
                           "progressEvery", "retryDelaySeconds", "maximumRetries"])
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys).isSubset(of: allowed) else { throw FixtureError.malformed }
        let config: Payload
        do { config = try JSONDecoder().decode(Payload.self, from: data) }
        catch { throw FixtureError.malformed }
        guard config.version == 1 else { throw FixtureError.invalidVersion }
        let aliasChars = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_")
        guard !config.alias.isEmpty,
              config.alias.unicodeScalars.allSatisfy(aliasChars.contains) else { throw FixtureError.invalidAlias }
        guard config.progressEvery > 0, config.retryDelaySeconds.isFinite,
              config.retryDelaySeconds > 0, config.maximumRetries >= 0 else { throw FixtureError.invalidControls }
        switch config.mode {
        case .empty:
            guard config.number == nil, config.count == nil, config.syntheticBase == nil else {
                throw FixtureError.unexpectedField
            }
        case .exact:
            guard let number = config.number, number.first == "+",
                  canonicalInteger(String(number.dropFirst())) != nil else { throw FixtureError.invalidNumber }
            guard config.count == nil, config.syntheticBase == nil else { throw FixtureError.unexpectedField }
        case .blocking, .identification:
            guard config.number == nil else { throw FixtureError.unexpectedField }
            guard let count = config.count, count >= 0 else { throw FixtureError.invalidCount }
            guard let rawBase = config.syntheticBase, let base = canonicalInteger(rawBase) else {
                throw FixtureError.invalidNumber
            }
            if count > 0 {
                guard let delta = Int64(exactly: count - 1), !base.addingReportingOverflow(delta).overflow else {
                    throw FixtureError.overflow
                }
            }
        }
        return Self(payload: config)
    }

    static func load(bundle: Bundle) throws -> Self {
        guard let url = bundle.url(forResource: "HarnessFixture", withExtension: "json") else {
            throw FixtureError.missing
        }
        let data: Data
        do { data = try Data(contentsOf: url) } catch { throw FixtureError.missing }
        return try decode(data)
    }

    private static func canonicalInteger(_ text: String) -> Int64? {
        guard !text.isEmpty, text.first != "0", text.utf8.allSatisfy({ (48...57).contains($0) }),
              let value = Int64(text), value > 0 else { return nil }
        return value
    }

    /// A bounded-memory stream. The fixture is validated before any OS mutation.
    func apply(to sink: DirectoryEntrySink, progress: (Int) -> Void = { _ in }) {
        if sink.isIncremental {
            sink.removeAllBlockingEntries()
            sink.removeAllIdentificationEntries()
        }
        switch mode {
        case .empty: break
        case .exact:
            // Only validated configurations are constructed by decode().
            sink.addBlockingEntry(withNextSequentialPhoneNumber: Int64(number!.dropFirst())!)
        case .blocking, .identification:
            let base = Int64(syntheticBase!)!
            for i in 0..<entryCount {
                autoreleasepool {
                    let number = base + Int64(i)
                    if mode == .identification {
                        sink.addIdentificationEntry(withNextSequentialPhoneNumber: number, label: "spam")
                    } else {
                        sink.addBlockingEntry(withNextSequentialPhoneNumber: number)
                    }
                }
                if i > 0 && i % progressEvery == 0 { progress(i) }
            }
        }
    }
}

protocol DirectoryEntrySink: AnyObject {
    var isIncremental: Bool { get }
    func removeAllBlockingEntries()
    func removeAllIdentificationEntries()
    func addBlockingEntry(withNextSequentialPhoneNumber number: Int64)
    func addIdentificationEntry(withNextSequentialPhoneNumber number: Int64, label: String)
}
