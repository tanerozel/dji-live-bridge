import CoreMediaIO
import Foundation

@main
enum CameraExtensionMain {
    static func main() {
        let providerSource = CameraExtensionProviderSource()
        CMIOExtensionProvider.startService(provider: providerSource.provider)
        CFRunLoopRun()
    }
}
