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

class CallDirectoryHandler: CXCallDirectoryProvider {

    override func beginRequest(with context: CXCallDirectoryExtensionContext) {
        let n = (Bundle.main.object(forInfoDictionaryKey: "LimitTestN") as? NSString)?.integerValue ?? 1_000_000
        let mode = (Bundle.main.object(forInfoDictionaryKey: "LimitTestMode") as? String) ?? "blocking"
        let incremental = context.isIncremental

        log.info("BEGIN n=\(n, privacy: .public) mode=\(mode, privacy: .public) incremental=\(incremental, privacy: .public) mem0=\(footprintMB(), privacy: .public)MB")

        // If iOS asks for an incremental update we must not re-add what is already
        // stored, or the insert hits a UNIQUE constraint (sqlite error 19).
        // Wipe first so every run measures a clean full load.
        if incremental {
            context.removeAllBlockingEntries()
            context.removeAllIdentificationEntries()
            log.info("incremental request: cleared existing entries first")
        }

        let t0 = Date()
        var last: Int64 = 0
        let base: Int64 = 100_000_000_000   // well above any real number, strictly ascending from here

        for i in 0..<n {
            autoreleasepool {
                let number = base + Int64(i)
                precondition(number > last, "NOT ASCENDING at \(i)")
                last = number
                if mode == "identification" {
                    context.addIdentificationEntry(withNextSequentialPhoneNumber: number, label: "spam")
                } else {
                    context.addBlockingEntry(withNextSequentialPhoneNumber: number)
                }
            }
            if i > 0 && i % 500_000 == 0 {
                log.info("PROGRESS i=\(i, privacy: .public) t=\(Date().timeIntervalSince(t0), privacy: .public)s mem=\(footprintMB(), privacy: .public)MB")
            }
        }

        let tGen = Date().timeIntervalSince(t0)
        log.info("GENERATED n=\(n, privacy: .public) in \(tGen, privacy: .public)s peakMem=\(footprintMB(), privacy: .public)MB — calling completeRequest")

        context.completeRequest { expired in
            let tAll = Date().timeIntervalSince(t0)
            log.info("COMPLETE n=\(n, privacy: .public) expired=\(expired, privacy: .public) total=\(tAll, privacy: .public)s mem=\(footprintMB(), privacy: .public)MB")
        }
    }
}
