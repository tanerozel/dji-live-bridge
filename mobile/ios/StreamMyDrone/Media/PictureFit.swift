import CoreGraphics

/// How the drone's picture sits on a screen of another shape.
enum PictureFit: String {
    /// Edge to edge when that cuts off little (a vertical picture on an upright phone, a wide one
    /// on a phone turned sideways), else the whole picture. Until the user picks one.
    case auto
    /// The whole picture; the rest of the screen shows its colors, blurred.
    case whole
    /// Edge to edge, like a camera app; the picture's sides (or top and bottom) are cut off.
    case fill

    static func fromStorage(_ value: String?) -> PictureFit {
        value.flatMap(PictureFit.init(rawValue:)) ?? .auto
    }
}

/// `PictureFit.auto` fills the screen while at least this share of the picture stays in view.
private let autoFillMinVisible: CGFloat = 0.75

/// What `fit` means for a picture of `aspect` (width / height) in an area of `width` by `height`.
func shownFit(width: CGFloat, height: CGFloat, aspect: CGFloat, fit: PictureFit) -> PictureFit {
    if fit != .auto { return fit }
    guard width > 0, height > 0 else { return .whole }
    let areaAspect = width / height
    let visible = aspect > areaAspect ? areaAspect / aspect : aspect / areaAspect
    return visible >= autoFillMinVisible ? .fill : .whole
}

/**
 * The picture's size in an area of `width` by `height` for a video of `aspect` (width / height):
 * as large as fits for `PictureFit.whole`, as small as covers it for `PictureFit.fill`.
 */
func pictureSize(width: CGFloat, height: CGFloat, aspect: CGFloat, fit: PictureFit) -> CGSize {
    let widerThanArea = aspect > width / height
    let fullWidth = shownFit(width: width, height: height, aspect: aspect, fit: fit) == .whole ? widerThanArea : !widerThanArea
    return fullWidth ? CGSize(width: width, height: width / aspect) : CGSize(width: height * aspect, height: height)
}
