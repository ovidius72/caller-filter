import Foundation

@main
struct ValidateFixture {
    static func main() {
        guard CommandLine.arguments.count == 2 else {
            fputs("Expected a fixture file path.\n", stderr)
            exit(2)
        }
        do {
            let data = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
            _ = try HarnessConfiguration.decode(data)
            print("Fixture transport validated (not numbering-plan validation).")
        } catch {
            // Never print source JSON, paths, numbers or decoder debug descriptions.
            fputs("Invalid fixture. Nothing was built or loaded.\n", stderr)
            exit(1)
        }
    }
}
