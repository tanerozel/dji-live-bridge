import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The themes, each with a small preview of its background and accent.
struct ThemePicker: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ChoiceList(title: tr("theme")) {
            ForEach(ThemeChoice.allCases) { choice in
                ChoiceRow(selected: model.theme == choice, title: tr(choice.labelKey), swatch: choice) {
                    model.setTheme(choice)
                }
            }
        } onDone: {
            dismiss()
        }
        .themed(model.theme)
    }
}

/// The phone's language or one of the app's; it applies at once.
struct LanguagePicker: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let current = Localizer.shared.language
        ChoiceList(title: tr("language")) {
            ChoiceRow(selected: current == nil, title: tr("language_system")) { Localizer.shared.setLanguage(nil) }
            ForEach(appLanguages) { language in
                ChoiceRow(selected: current == language, title: language.name) { Localizer.shared.setLanguage(language) }
            }
        } onDone: {
            dismiss()
        }
        .themed(model.theme)
    }
}

private struct ChoiceList<Rows: View>: View {
    let title: String
    @ViewBuilder let rows: Rows
    let onDone: () -> Void
    @Environment(\.palette) private var palette

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 0) { rows }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
            .background(palette.card.ignoresSafeArea())
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(tr("ok"), action: onDone) } }
        }
        .presentationDetents([.medium, .large])
    }
}

private struct ChoiceRow: View {
    let selected: Bool
    let title: String
    var swatch: ThemeChoice?
    let action: () -> Void
    @Environment(\.palette) private var palette

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                if let swatch { ThemeSwatch(choice: swatch) }
                Text(title).font(.body).foregroundStyle(palette.text).frame(maxWidth: .infinity, alignment: .leading)
                Image(systemName: selected ? "largecircle.fill.circle" : "circle")
                    .foregroundStyle(selected ? palette.accent : palette.faint)
                    .font(.title3)
            }
            .frame(minHeight: 52)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// A small preview of a theme: its background with its accent in the middle.
private struct ThemeSwatch: View {
    let choice: ThemeChoice
    @Environment(\.palette) private var palette
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        ZStack {
            if choice == .system {
                HStack(spacing: 0) {
                    BridgePalette.light.background
                    BridgePalette.dark.background
                }
                .clipShape(Circle())
                Circle().fill(BridgePalette.light.accent).frame(width: 10, height: 10)
            } else {
                let swatch = BridgePalette.forChoice(choice, systemDark: scheme == .dark)
                Circle().fill(swatch.background)
                Circle().fill(swatch.accent).frame(width: 10, height: 10)
            }
            Circle().strokeBorder(palette.border, lineWidth: 1)
        }
        .frame(width: 28, height: 28)
    }
}

/// A picked video, copied where the app can read it after the picker closes.
struct PickedVideo {
    let url: URL
    let name: String?
}

/**
 * The system photo picker, limited to videos. It runs outside the app, so no photo library
 * permission is needed; the video is copied into the app's caches, as the original (an HEVC or
 * HDR video is converted by the app itself, the way DJI Fly would send it).
 */
struct VideoPicker: UIViewControllerRepresentable {
    /// A video was chosen and is being copied; the picker can close.
    let onStart: () -> Void
    let onPicked: (PickedVideo?) -> Void

    func makeUIViewController(context: Context) -> PHPickerViewController {
        var configuration = PHPickerConfiguration()
        configuration.filter = .videos
        configuration.selectionLimit = 1
        configuration.preferredAssetRepresentationMode = .current
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: PHPickerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onStart: onStart, onPicked: onPicked) }

    final class Coordinator: NSObject, PHPickerViewControllerDelegate {
        private let onStart: () -> Void
        private let onPicked: (PickedVideo?) -> Void

        init(onStart: @escaping () -> Void, onPicked: @escaping (PickedVideo?) -> Void) {
            self.onStart = onStart
            self.onPicked = onPicked
        }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            guard let provider = results.first?.itemProvider,
                  provider.hasItemConformingToTypeIdentifier(UTType.movie.identifier) else {
                onPicked(nil)
                return
            }
            let name = provider.suggestedName
            onStart()
            provider.loadFileRepresentation(forTypeIdentifier: UTType.movie.identifier) { [onPicked] url, _ in
                // The picker deletes its file when this returns; the app keeps a copy.
                let copy = url.flatMap { source -> URL? in
                    let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                        .appendingPathComponent("picked-video", isDirectory: true)
                    try? FileManager.default.removeItem(at: directory)
                    try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                    let target = directory.appendingPathComponent(source.lastPathComponent)
                    return (try? FileManager.default.copyItem(at: source, to: target)) != nil ? target : nil
                }
                let displayName = name.map { name in copy.map { "\(name).\($0.pathExtension)" } ?? name }
                DispatchQueue.main.async {
                    onPicked(copy.map { PickedVideo(url: $0, name: displayName) })
                }
            }
        }
    }
}
