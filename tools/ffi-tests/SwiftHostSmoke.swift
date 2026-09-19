import Foundation

func equal<T: Equatable>(_ actual: T, _ expected: T) {
    precondition(actual == expected, "got \(actual), expected \(expected)")
}
func expectError(_ expected: FfiError, _ body: () throws -> Void) {
    do { try body(); fatalError("expected \(expected)") }
    catch let error as FfiError { equal(error, expected) }
    catch { fatalError("unexpected error \(error)") }
}

// Tests call each sink synchronously on a single thread. Reentry does not mutate it.
final class TestSink: ExpansionSink, @unchecked Sendable {
    var batches: [[Int64]] = []
    let action: () throws -> ExpansionStatus
    init(_ action: @escaping () throws -> ExpansionStatus = { .continue }) { self.action = action }
    func onBatch(values: [Int64]) throws -> ExpansionStatus {
        batches.append(values)
        return try action()
    }
}
struct Unexpected: Error {}

@main
struct SwiftHostSmoke {
    static func main() throws {
        func fixture(_ name: String) throws -> Data {
            try Data(contentsOf: URL(fileURLWithPath: "target/ffi-tests/fixtures/\(name)"))
        }
        func exact(_ id: UInt64, _ effect: EffectInput, _ digits: String) -> RuleInput {
            RuleInput(id: id, effect: effect, matcher: .exact(digits: digits))
        }
        let bytes = try fixture("numbering.cfnd")
        let snapshot = try Snapshot(numbering: bytes, places: [fixture("places-user.cfds"), fixture("places-en.cfds")])
        equal(coreVersion(), "0.1.0")
        equal(snapshot.numberingUpstream(), "future-test-version")
        equal(snapshot.numberMetadataFormatVersion(), 1)
        equal(snapshot.datasetFormatVersion(), 1)
        equal(snapshot.datasetVersions().count, 2)
        equal(snapshot.placePrefixes(name: "Test Town", language: "en"), ["3902", "3903"])
        equal(snapshot.placeNames(language: "en"), ["Test Town"])
        equal(try normalizeNumber(raw: "0200000000", defaultRegion: "IT", snapshot: snapshot).e164, "+390200000000")
        equal(try normalizeNumber(raw: "+39 0200000000", defaultRegion: "absent", snapshot: snapshot).e164, "+390200000000")
        equal(geocodeNumber(number: "+390200000000", snapshot: snapshot), .place(name: "Test Town", language: "en"))
        equal(geocodeNumber(number: "+390400000000", snapshot: snapshot), .place(name: "Test Town", language: "test-language"))
        equal(geocodeNumber(number: "+393000000000", snapshot: snapshot), .notGeographic)
        equal(geocodeNumber(number: "+390500000000", snapshot: snapshot), .noData)
        equal(geocodeNumber(number: "bad", snapshot: snapshot), .notANumber)
        precondition(isNumberGeographic(number: "+390200000000", snapshot: snapshot))
        let empty = try Snapshot(numbering: fixture("empty.cfnd"), places: [])
        expectError(.Normalize(reason: .regionNotLoaded)) {
            _ = try normalizeNumber(raw: "0200000000", defaultRegion: "IT", snapshot: empty)
        }
        expectError(.NumberMetadata(reason: .badMagic)) { _ = try Snapshot(numbering: Data("oops".utf8), places: []) }
        expectError(.Dataset(reason: .badMagic)) { _ = try Snapshot(numbering: bytes, places: [Data("oops".utf8)]) }
        // Previously loaded snapshot remains usable after rejected replacements.
        equal(snapshot.numberingUpstream(), "future-test-version")

        let first: UInt64 = UInt64.max - 1
        let rules = try PreparedRules(inputs: [exact(first, .deny, "390200000000"), exact(2, .deny, "390200000001")])
        equal(try evaluateNumber(number: "+390200000000", callerId: nil, rules: rules).matchedRule, first)
        equal(try evaluateNumber(number: "+390200000000", callerId: nil, rules: rules).decision, .block)
        let conflicts = try PreparedRules(inputs: [exact(1, .deny, "123"), exact(2, .allow, "123")])
        equal(conflicts.conflicts(), [ConflictOutput(first: 1, second: 2)])
        equal(try evaluateNumber(number: "123", callerId: nil, rules: conflicts).contested, true)
        expectError(.DuplicateRuleId(id: 1)) { _ = try PreparedRules(inputs: [exact(1, .deny, "123"), exact(1, .deny, "456")]) }
        expectError(.InvalidNumber) { _ = try evaluateNumber(number: "bad", callerId: nil, rules: rules) }
        expectError(.Rule(reason: .patternPinsNothing)) { _ = try PreparedRules(inputs: [RuleInput(id: 1, effect: .deny, matcher: .pattern(value: "xxx"))]) }

        let live = SurfaceInput(id: "live", evaluatesLive: true, matchesCallerId: true, budget: nil)
        let list = SurfaceInput(id: "list", evaluatesLive: false, matchesCallerId: false, budget: BudgetInput(maxEntries: 2, maxCandidates: 10))
        let explanation = try explainRule(ruleId: first, rules: rules, snapshot: snapshot, surfaces: [live, list])
        equal(explanation.verdicts.map { $0.verdict }, [.appliesLive, .fits(entries: 1)])
        let budget = BudgetInput(maxEntries: 2, maxCandidates: 10)
        let sink = TestSink { equal(rules.len(), 2); return .continue } // reentrant Rust call
        equal(try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: 1, sink: sink), .fits(entries: 2))
        equal(sink.batches, [[390200000000], [390200000001]])
        let never = TestSink()
        equal(try expandRulesBatched(rules: rules, snapshot: snapshot, budget: BudgetInput(maxEntries: 1, maxCandidates: 10), batchSize: 1, sink: never),
              .tooBroad(upperBound: 2, exact: true, ruleIds: [2, first]))
        precondition(never.batches.isEmpty)
        let cancel = TestSink { .cancel }
        equal(try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: 1, sink: cancel), .cancelled(entries: 1))
        equal(cancel.batches.count, 1)
        expectError(.Callback(reason: "stop")) {
            _ = try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: 1, sink: TestSink { throw FfiError.Callback(reason: "stop") })
        }
        do {
            _ = try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: 1, sink: TestSink { throw Unexpected() })
            fatalError("expected unexpected callback error")
        } catch FfiError.UnexpectedCallback(let reason) { precondition(!reason.isEmpty) }
        expectError(.InvalidBatchSize) { _ = try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: 0, sink: never) }
        equal(try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: UInt32.max, sink: TestSink()), .fits(entries: 2))

        let carved = try PreparedRules(inputs: [RuleInput(id: 1, effect: .deny, matcher: .pattern(value: "39020000000x")), exact(2, .allow, "390200000008")])
        let carvedSink = TestSink()
        equal(try expandRulesBatched(rules: carved, snapshot: snapshot, budget: BudgetInput(maxEntries: 9, maxCandidates: 10), batchSize: 2, sink: carvedSink), .fits(entries: 9))
        let numbers = carvedSink.batches.flatMap { $0 }
        equal(numbers.count, 9)
        precondition(!numbers.contains(390200000008))
        precondition(zip(numbers, numbers.dropFirst()).allSatisfy { $0 < $1 })
        let suffix = try PreparedRules(inputs: [RuleInput(id: 3, effect: .deny, matcher: .suffix(digits: "123"))])
        equal(try expandRulesBatched(rules: suffix, snapshot: snapshot, budget: budget, batchSize: 1, sink: never), .notExpandable(ruleId: 3, reason: .suffix))
        let location = try prepareRulesForSnapshot(inputs: [RuleInput(id: 5, effect: .deny, matcher: .location(name: "Test Town", language: "en"))], snapshot: snapshot)
        equal(try explainRule(ruleId: 5, rules: location, snapshot: snapshot, surfaces: [live]).caveats, [.landlinesOnly])
        let changed = try Snapshot(numbering: bytes, places: [fixture("places-new.cfds")])
        expectError(.SnapshotMismatch) { _ = try explainRule(ruleId: 5, rules: location, snapshot: changed, surfaces: [live]) }
        expectError(.SnapshotMismatch) { _ = try expandRulesBatched(rules: location, snapshot: changed, budget: budget, batchSize: 1, sink: never) }
        DispatchQueue.concurrentPerform(iterations: 20) { _ in
            equal(try! evaluateNumber(number: "+390200000000", callerId: nil, rules: rules).decision, .block)
        }
        weak var released: TestSink?
        do {
            let temporary = TestSink(); released = temporary
            _ = try expandRulesBatched(rules: rules, snapshot: snapshot, budget: budget, batchSize: 1, sink: temporary)
        }
        precondition(released == nil, "Rust must release foreign callback ownership")
        print("Swift FFI: data, rules, errors, snapshots, callbacks, cancellation, reentry and threads passed")
    }
}
