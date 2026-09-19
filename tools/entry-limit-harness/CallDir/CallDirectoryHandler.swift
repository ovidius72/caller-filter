import Foundation
import CallKit
import os
import Darwin

let log = Logger(subsystem: "com.antoniopantano.limittest", category: "extension")

func footprintMB() -> Double {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
        }
    }
    guard kr == KERN_SUCCESS else { return -1 }
    return Double(info.phys_footprint) / 1024.0 / 1024.0
}

extension CXCallDirectoryExtensionContext: DirectoryEntrySink {}

class CallDirectoryHandler: CXCallDirectoryProvider {
    override func beginRequest(with context: CXCallDirectoryExtensionContext) {
        let config: HarnessConfiguration
        do {
            // Decode before clearing any existing entries. No missing-fixture fallback.
            config = try HarnessConfiguration.load(bundle: .main)
        } catch {
            log.error("Fixture rejected; no entries submitted")
            context.cancelRequest(withError: NSError(domain: "HarnessFixture", code: 1))
            return
        }
        let n = config.entryCount
        let mode = config.mode.rawValue
        log.info("BEGIN n=\(n, privacy: .public) mode=\(mode, privacy: .public) incremental=\(context.isIncremental, privacy: .public) mem0=\(footprintMB(), privacy: .public)MB")
        let t0 = Date()
        config.apply(to: context) { i in
            log.info("PROGRESS i=\(i, privacy: .public) t=\(Date().timeIntervalSince(t0), privacy: .public)s mem=\(footprintMB(), privacy: .public)MB")
        }
        log.info("GENERATED n=\(n, privacy: .public) in \(Date().timeIntervalSince(t0), privacy: .public)s currentMem=\(footprintMB(), privacy: .public)MB — calling completeRequest")
        context.completeRequest { expired in
            log.info("COMPLETE n=\(n, privacy: .public) expired=\(expired, privacy: .public) total=\(Date().timeIntervalSince(t0), privacy: .public)s mem=\(footprintMB(), privacy: .public)MB")
        }
    }
}
