import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    styleMask.insert(.fullSizeContentView)
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    isMovableByWindowBackground = true
    if #available(macOS 11.0, *) {
      titlebarSeparatorStyle = .none
    }

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
