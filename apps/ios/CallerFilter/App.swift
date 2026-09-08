import SwiftUI

@main
struct CallerFilterApp: App {
    var body: some Scene {
        WindowGroup { ContentView() }
    }
}

struct ContentView: View {
    var body: some View {
        VStack(spacing: 12) {
            Text("Caller Filter").font(.headline)
            // Proves Swift is talking to the Rust core rather than a stale copy.
            Text("core \(coreVersion())")
                .font(.system(.body, design: .monospaced))
            Text("iOS entry limit \(defaultEntryLimit())")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)
        }
        .padding()
    }
}
