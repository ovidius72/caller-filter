import SwiftUI
import CallKit
import os

let log = Logger(subsystem: "com.antoniopantano.limittest", category: "harness")

@main
struct LimitTestApp: App {
    var body: some Scene { WindowGroup { ContentView() } }
}

struct ContentView: View {
    @Environment(\.scenePhase) private var scenePhase
    @State private var state = HarnessLoadState()
    @State private var fixture: HarnessConfiguration?
    @State private var status = "No reload verified in this app session."
    @State private var fixtureError: String?
    @State private var statusRequest = UUID()
    @State private var capacityAutoReloadPending = false
    private let extID = "com.antoniopantano.limittest.calldir"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text(fixture?.isCapacityTest == true ? "Call Directory capacity test" : "Silent-blocking probe")
                    .font(.headline).accessibilityAddTraits(.isHeader)
                if let fixture {
                    Text("Fixture: \(fixture.alias)")
                    Text("Mode: \(fixture.mode.rawValue) · Entries: \(fixture.entryCount)")
                    if fixture.mode == .empty { Text("Empty control: this extension should block no numbers.") }
                }
                if let fixtureError { Text(fixtureError).foregroundStyle(.red) }
                Text("Extension: \(state.enablement.rawValue)")
                Text(state.readyForTrial ? "List loaded. Ready for manual observation." : "Not ready for a blocked-call trial.")
                    .fontWeight(.semibold)
                Text(status).font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                if state.isLoading { ProgressView("Reloading; do not place test calls yet.") }
                Button("Reload bundled list") { beginReload() }
                    .buttonStyle(.borderedProminent)
                    .disabled(fixture == nil || state.isLoading || state.enablement != .enabled)
                Button("Refresh extension status") { refreshStatus() }
                    .disabled(state.isLoading)
                Button("Open Phone settings") {
                    CXCallDirectoryManager.sharedInstance.openSettings { error in
                        if let error {
                            DispatchQueue.main.async {
                                status = "Could not open settings (code \((error as NSError).code))."
                            }
                        }
                    }
                }.disabled(state.isLoading)
                Text("Enable this extension in Phone → Call Blocking & Identification. To clear the test block, install the empty fixture and reload it; or disable this extension in Settings.")
                Text("Reload events are not call history. iOS does not report blocked calls to this app.")
                    .font(.footnote)
            }.padding()
        }
        .onAppear {
            do { fixture = try HarnessConfiguration.load(bundle: .main) }
            catch {
                fixtureError = (error as? HarnessConfiguration.FixtureError)?.errorDescription ?? "Cannot read fixture."
            }
            capacityAutoReloadPending = fixture?.isCapacityTest == true
            refreshStatus()
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active && !state.isLoading { refreshStatus() }
        }
    }

    private func refreshStatus() {
        let request = UUID()
        statusRequest = request
        state.setEnablement(.checking)
        CXCallDirectoryManager.sharedInstance.getEnabledStatusForExtension(withIdentifier: extID) { value, error in
            DispatchQueue.main.async {
                guard statusRequest == request else { return }
                if error != nil { state.setEnablement(.unknown) }
                else {
                    switch value {
                    case .enabled: state.setEnablement(.enabled)
                    case .disabled: state.setEnablement(.disabled)
                    case .unknown: state.setEnablement(.unknown)
                    @unknown default: state.setEnablement(.unknown)
                    }
                }
                if let error { status = "Status query failed (code \((error as NSError).code))." }
                if capacityAutoReloadPending && state.enablement == .enabled {
                    capacityAutoReloadPending = false
                    beginReload()
                }
            }
        }
    }

    private func beginReload() {
        guard fixture != nil, state.beginReload() else { return }
        // Retire any older status callback while the single reload is in flight.
        statusRequest = UUID()
        reload(retry: 0, started: Date())
    }

    private func reload(retry: Int, started: Date) {
        guard let fixture else { state.finishReload(success: false); return }
        status = "Reloading \(fixture.mode.rawValue), attempt \(retry + 1)…"
        CXCallDirectoryManager.sharedInstance.reloadExtension(withIdentifier: extID) { error in
            DispatchQueue.main.async {
                if let error = error as NSError?,
                   error.domain == CXErrorDomainCallDirectoryManager, error.code == 7,
                   retry < fixture.maximumRetries {
                    status = "iOS is already loading. Retry \(retry + 1)/\(fixture.maximumRetries)."
                    DispatchQueue.main.asyncAfter(deadline: .now() + fixture.retryDelaySeconds) {
                        reload(retry: retry + 1, started: started)
                    }
                    return
                }
                state.finishReload(success: error == nil)
                let seconds = String(format: "%.2f", Date().timeIntervalSince(started))
                // Never include localizedDescription: an OS SQL error may contain a number.
                let outcome = error.map { "FAIL code=\(($0 as NSError).code)" } ?? "OK"
                let event = "\(ISO8601DateFormatter().string(from: Date())) mode=\(fixture.mode.rawValue) n=\(fixture.entryCount) RELOAD=\(outcome) t=\(seconds)s"
                status = event
                append(event)
                log.info("\(event, privacy: .public)")
                // Re-check enablement after completion; success alone is not ready.
                refreshStatus()
            }
        }
    }

    private func append(_ line: String) {
        guard let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else {
            status += "\nCould not locate the local reload log."
            return
        }
        let url = directory.appendingPathComponent("results.txt")
        do {
            if FileManager.default.fileExists(atPath: url.path) {
                let handle = try FileHandle(forWritingTo: url)
                defer { try? handle.close() }
                try handle.seekToEnd()
                try handle.write(contentsOf: Data((line + "\n").utf8))
            } else { try Data((line + "\n").utf8).write(to: url, options: .atomic) }
        } catch { status += "\nCould not save the local reload log." }
    }
}
