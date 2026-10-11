import AppKit
import Foundation

private final class DeckCraftLauncher: NSObject, NSApplicationDelegate {
    private let supportedExtensions: Set<String> = ["deckcraft", "pptx", "potx", "ppsx"]
    private var pendingFiles: [String] = []
    private var process: Process?
    private var input: FileHandle?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let executable = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/DeckCraftUI")
        let child = Process()
        let pipe = Pipe()
        child.executableURL = executable
        child.arguments = Array(CommandLine.arguments.dropFirst())
        child.standardInput = pipe
        child.environment = ProcessInfo.processInfo.environment.merging(["DECKCRAFT_OPEN_STDIN": "1"]) { _, new in new }
        child.terminationHandler = { _ in
            DispatchQueue.main.async {
                NSApp.terminate(nil)
            }
        }

        do {
            try child.run()
            process = child
            input = pipe.fileHandleForWriting
            forwardPendingFiles()
        } catch {
            NSLog("DeckCraft could not launch its UI: %@", error.localizedDescription)
            let alert = NSAlert(error: error)
            alert.messageText = "DeckCraft could not start"
            alert.runModal()
            NSApp.terminate(nil)
        }
    }

    func application(_ sender: NSApplication, openFiles filenames: [String]) {
        let supported = filenames.filter {
            supportedExtensions.contains(URL(fileURLWithPath: $0).pathExtension.lowercased())
        }
        pendingFiles.append(contentsOf: supported)
        forwardPendingFiles()
        sender.reply(toOpenOrPrint: supported.count == filenames.count ? .success : .failure)
    }

    private func forwardPendingFiles() {
        guard let input, !pendingFiles.isEmpty else { return }
        do {
            var message = try JSONSerialization.data(withJSONObject: pendingFiles)
            message.append(0x0A)
            try input.write(contentsOf: message)
            pendingFiles.removeAll()
        } catch {
            NSLog("DeckCraft could not forward opened files: %@", error.localizedDescription)
        }
    }
}

private let application = NSApplication.shared
private let launcher = DeckCraftLauncher()
application.delegate = launcher
application.setActivationPolicy(.accessory)
application.run()
