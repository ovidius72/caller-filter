import Foundation

final class RecordingSink: DirectoryEntrySink {
    let isIncremental: Bool
    var events: [String] = []
    var blocked: [Int64] = []
    var identified: [Int64] = []
    init(incremental: Bool) { isIncremental = incremental }
    func removeAllBlockingEntries() { events.append("clear-block"); blocked.removeAll() }
    func removeAllIdentificationEntries() { events.append("clear-label"); identified.removeAll() }
    func addBlockingEntry(withNextSequentialPhoneNumber number: Int64) {
        events.append("block"); blocked.append(number)
    }
    func addIdentificationEntry(withNextSequentialPhoneNumber number: Int64, label: String) {
        precondition(label == "spam")
        events.append("label"); identified.append(number)
    }
}

@main
struct ProbeTests {
    static func main() throws {
        let defaultsData = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
        let defaults = try JSONSerialization.jsonObject(with: defaultsData) as! [String: Any]
        let rawNumber = defaults["syntheticBase"] as! String // Synthetic test data, never dialled.
        let number = Int64(rawNumber)!
        let empty: [String: Any] = ["version": 1, "alias": "test", "mode": "empty",
                                   "progressEvery": defaults["progressEvery"]!,
                                   "retryDelaySeconds": defaults["retryDelaySeconds"]!,
                                   "maximumRetries": defaults["maximumRetries"]!]
        func decode(_ value: [String: Any]) throws -> HarnessConfiguration {
            try HarnessConfiguration.decode(JSONSerialization.data(withJSONObject: value))
        }
        var rejected = 0
        func reject(_ value: [String: Any]) {
            do { _ = try decode(value); preconditionFailure("Invalid fixture accepted") }
            catch { rejected += 1 }
        }
        func changed(_ key: String, _ value: Any) -> [String: Any] {
            var result = empty; result[key] = value; return result
        }
        reject(changed("version", 2))
        reject(changed("version", true))
        reject(changed("mode", "typo"))
        reject(changed("alias", "bad\nalias"))
        reject(changed("alias", ""))
        reject(changed("progressEvery", 0))
        reject(changed("retryDelaySeconds", 0))
        reject(changed("maximumRetries", -1))
        reject(changed("count", 1))
        reject(changed("number", "+" + rawNumber))
        reject(changed("unexpected", true))
        var missing = empty; missing.removeValue(forKey: "mode"); reject(missing)
        do { _ = try HarnessConfiguration.decode(Data("not json".utf8)); preconditionFailure() }
        catch { rejected += 1 }

        var exact = empty
        exact["mode"] = "exact"
        exact["number"] = "+" + rawNumber
        for bad in ["", rawNumber, "+", "+0" + rawNumber, "+  " + rawNumber,
                    "+" + rawNumber + " extension", "+" + rawNumber + "\n",
                    "+" + String(Int64.max) + "0", "+１２３"] {
            var invalid = exact; invalid["number"] = bad; reject(invalid)
        }
        var mixed = exact; mixed["count"] = 1; reject(mixed)
        for incremental in [false, true] {
            let sink = RecordingSink(incremental: incremental)
            try decode(exact).apply(to: sink)
            precondition(sink.blocked == [number] && sink.identified.isEmpty)
            precondition(sink.events == (incremental ? ["clear-block", "clear-label", "block"] : ["block"]))
            let emptySink = RecordingSink(incremental: incremental)
            try decode(empty).apply(to: emptySink)
            precondition(emptySink.events == (incremental ? ["clear-block", "clear-label"] : []))
        }
        // Repeated incremental reloads must replace, not accumulate, entries/labels.
        let reused = RecordingSink(incremental: true)
        try decode(exact).apply(to: reused)
        try decode(exact).apply(to: reused)
        precondition(reused.blocked == [number])
        try decode(empty).apply(to: reused)
        precondition(reused.blocked.isEmpty && reused.identified.isEmpty)

        for mode in ["blocking", "identification"] {
            var capacity = empty
            capacity["mode"] = mode; capacity["count"] = 4
            capacity["syntheticBase"] = rawNumber; capacity["progressEvery"] = 2
            let sink = RecordingSink(incremental: true)
            var progress: [Int] = []
            try decode(capacity).apply(to: sink) { progress.append($0) }
            let entries = mode == "blocking" ? sink.blocked : sink.identified
            precondition(entries == (0..<4).map { number + Int64($0) })
            precondition(progress == [2])
            precondition(sink.events.prefix(2) == ["clear-block", "clear-label"])
            capacity["count"] = -1; reject(capacity)
            capacity["count"] = 2; capacity["syntheticBase"] = String(Int64.max); reject(capacity)
            capacity["count"] = 0
            let zero = RecordingSink(incremental: true)
            try decode(capacity).apply(to: zero)
            precondition(zero.events == ["clear-block", "clear-label"])
        }

        var state = HarnessLoadState()
        precondition(!state.readyForTrial && !state.beginReload())
        state.setEnablement(.unknown); precondition(!state.beginReload())
        state.setEnablement(.disabled); precondition(!state.beginReload())
        state.setEnablement(.enabled); precondition(!state.readyForTrial)
        precondition(state.beginReload() && !state.beginReload())
        precondition(!state.readyForTrial)
        state.finishReload(success: true); precondition(state.readyForTrial)
        state.setEnablement(.checking); precondition(!state.readyForTrial)
        state.setEnablement(.enabled); precondition(state.readyForTrial)
        state.setEnablement(.disabled); state.setEnablement(.enabled)
        precondition(!state.readyForTrial)
        precondition(state.beginReload()); state.finishReload(success: false)
        precondition(!state.readyForTrial && !state.isLoading)
        precondition(state.beginReload()); state.setEnablement(.unknown)
        state.finishReload(success: true); precondition(!state.readyForTrial)
        print("PASS Swift probe: \(rejected) invalid fixtures rejected; full/incremental exact, empty, capacity, recovery and load states")
    }
}
