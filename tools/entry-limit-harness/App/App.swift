import SwiftUI
import CallKit
import os

let log = Logger(subsystem: "com.antoniopantano.limittest", category: "harness")

@main
struct LimitTestApp: App {
    var body: some Scene { WindowGroup { ContentView() } }
}

struct ContentView: View {
    @State private var status = "starting…"
    private let extID = "com.antoniopantano.limittest.calldir"

    var body: some View {
        VStack(spacing: 16) {
            Text("Call Directory limit test").font(.headline)
            Text(status).font(.system(.body, design: .monospaced))
                .multilineTextAlignment(.center).padding()
            Button("Run again") { run() }.buttonStyle(.borderedProminent)
        }
        .padding()
        .onAppear { run() }          // auto-run on launch
    }

    func resultsURL() -> URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("results.txt")
    }

    func append(_ line: String) {
        let url = resultsURL()
        let data = (line + "\n").data(using: .utf8)!
        if let h = try? FileHandle(forWritingTo: url) {
            h.seekToEndOfFile(); h.write(data); try? h.close()
        } else {
            try? data.write(to: url)
        }
    }

    func run(retry: Int = 0) {
        var n = "?", mode = "?"
        if let ext = Bundle.main.builtInPlugInsURL
            .flatMap({ try? FileManager.default.contentsOfDirectory(at: $0, includingPropertiesForKeys: nil) })?
            .first(where: { $0.pathExtension == "appex" }),
           let b = Bundle(url: ext) {
            n = (b.object(forInfoDictionaryKey: "LimitTestN") as? String) ?? "?"
            mode = (b.object(forInfoDictionaryKey: "LimitTestMode") as? String) ?? "?"
        }
        let t0 = Date()
        status = "reloading n=\(n)…"
        CXCallDirectoryManager.sharedInstance.reloadExtension(withIdentifier: extID) { err in
            let dt = Date().timeIntervalSince(t0)
            let line: String
            // code 7 = CurrentlyLoading: iOS is still busy, wait and retry
            if let e = err as NSError?, e.code == 7, retry < 40 {
                log.info("currentlyLoading, retry \(retry, privacy: .public)")
                DispatchQueue.main.async { status = "busy, retrying (\(retry + 1))…" }
                DispatchQueue.main.asyncAfter(deadline: .now() + 10) { run(retry: retry + 1) }
                return
            }
            if let e = err as NSError? {
                line = "n=\(n) mode=\(mode) RESULT=FAIL code=\(e.code) t=\(String(format: "%.2f", dt))s desc=\(e.localizedDescription)"
            } else {
                line = "n=\(n) mode=\(mode) RESULT=OK t=\(String(format: "%.2f", dt))s"
            }
            log.info("\(line, privacy: .public)")
            append(line)
            DispatchQueue.main.async { status = line }
        }
    }
}
