import CoreGraphics
import Foundation
import ImageIO

private struct ImageMetrics: Codable {
    let width: Int
    let height: Int
    let foregroundDensity: Double
    let foregroundCentroidX: Double
    let foregroundCentroidY: Double
    let edgeDensity: Double
    let rowOccupancy: [Double]
}

private struct Comparison: Codable {
    let native: ImageMetrics
    let web: ImageMetrics
    let rowCorrelation: Double
    let rowTransportDistance: Double
    let centroidDelta: Double
    let edgeDensityRatio: Double
    let passed: Bool
}

private enum ComparisonError: Error, CustomStringConvertible {
    case usage
    case unreadable(String)
    case invalidDimensions(String)

    var description: String {
        switch self {
        case .usage:
            return "usage: phase4-visual-compare NATIVE.png WEB.png"
        case let .unreadable(path):
            return "cannot decode '\(path)'"
        case let .invalidDimensions(path):
            return "'\(path)' does not have positive image dimensions"
        }
    }
}

private let sampleWidth = 84
private let sampleHeight = 156

private func load(_ path: String) throws -> (CGImage, Int, Int) {
    let url = URL(fileURLWithPath: path) as CFURL
    guard let source = CGImageSourceCreateWithURL(url, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        throw ComparisonError.unreadable(path)
    }
    guard image.width > 0, image.height > 0 else {
        throw ComparisonError.invalidDimensions(path)
    }
    return (image, image.width, image.height)
}

private func pixels(_ image: CGImage) -> [UInt8] {
    var bytes = [UInt8](repeating: 0, count: sampleWidth * sampleHeight * 4)
    bytes.withUnsafeMutableBytes { buffer in
        guard let context = CGContext(
            data: buffer.baseAddress,
            width: sampleWidth,
            height: sampleHeight,
            bitsPerComponent: 8,
            bytesPerRow: sampleWidth * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            return
        }
        context.interpolationQuality = .medium
        context.draw(
            image,
            in: CGRect(x: 0, y: 0, width: sampleWidth, height: sampleHeight)
        )
    }
    return bytes
}

private func luminance(_ bytes: [UInt8], _ index: Int) -> Double {
    let offset = index * 4
    return (
        0.2126 * Double(bytes[offset])
            + 0.7152 * Double(bytes[offset + 1])
            + 0.0722 * Double(bytes[offset + 2])
    ) / 255.0
}

private func dominantLuminance(_ bytes: [UInt8]) -> Double {
    var bins = [Int](repeating: 0, count: 32)
    for index in 0..<(sampleWidth * sampleHeight) {
        let bin = min(31, Int(luminance(bytes, index) * 32.0))
        bins[bin] += 1
    }
    let dominant = bins.indices.max(by: { bins[$0] < bins[$1] }) ?? 0
    return (Double(dominant) + 0.5) / 32.0
}

private func metrics(_ path: String) throws -> ImageMetrics {
    let (image, width, height) = try load(path)
    let bytes = pixels(image)
    let background = dominantLuminance(bytes)
    var foreground = 0
    var sumX = 0.0
    var sumY = 0.0
    var edges = 0
    var rowCounts = [Int](repeating: 0, count: 12)

    for y in 0..<sampleHeight {
        for x in 0..<sampleWidth {
            let index = y * sampleWidth + x
            let value = luminance(bytes, index)
            if abs(value - background) > 0.14 {
                foreground += 1
                sumX += Double(x) / Double(sampleWidth - 1)
                sumY += Double(y) / Double(sampleHeight - 1)
                rowCounts[min(11, y * 12 / sampleHeight)] += 1
            }
            if x + 1 < sampleWidth, y + 1 < sampleHeight {
                let horizontal = abs(value - luminance(bytes, index + 1))
                let vertical = abs(value - luminance(bytes, index + sampleWidth))
                if max(horizontal, vertical) > 0.16 { edges += 1 }
            }
        }
    }
    let total = Double(sampleWidth * sampleHeight)
    let count = Double(max(1, foreground))
    let rowArea = Double(sampleWidth * sampleHeight / 12)
    return ImageMetrics(
        width: width,
        height: height,
        foregroundDensity: Double(foreground) / total,
        foregroundCentroidX: sumX / count,
        foregroundCentroidY: sumY / count,
        edgeDensity: Double(edges) / total,
        rowOccupancy: rowCounts.map { Double($0) / rowArea }
    )
}

private func correlation(_ left: [Double], _ right: [Double]) -> Double {
    let leftMean = left.reduce(0, +) / Double(left.count)
    let rightMean = right.reduce(0, +) / Double(right.count)
    var numerator = 0.0
    var leftSquare = 0.0
    var rightSquare = 0.0
    for index in left.indices {
        let leftDelta = left[index] - leftMean
        let rightDelta = right[index] - rightMean
        numerator += leftDelta * rightDelta
        leftSquare += leftDelta * leftDelta
        rightSquare += rightDelta * rightDelta
    }
    let denominator = (leftSquare * rightSquare).squareRoot()
    return denominator == 0 ? 0 : numerator / denominator
}

private func transportDistance(_ left: [Double], _ right: [Double]) -> Double {
    let leftTotal = max(0.000_001, left.reduce(0, +))
    let rightTotal = max(0.000_001, right.reduce(0, +))
    var cumulative = 0.0
    var distance = 0.0
    for index in left.indices {
        cumulative += left[index] / leftTotal - right[index] / rightTotal
        distance += abs(cumulative)
    }
    return distance / Double(left.count)
}

private func run() throws {
    guard CommandLine.arguments.count == 3 else { throw ComparisonError.usage }
    let native = try metrics(CommandLine.arguments[1])
    let web = try metrics(CommandLine.arguments[2])
    let rowCorrelation = correlation(native.rowOccupancy, web.rowOccupancy)
    let rowTransportDistance = transportDistance(native.rowOccupancy, web.rowOccupancy)
    let centroidDelta = hypot(
        native.foregroundCentroidX - web.foregroundCentroidX,
        native.foregroundCentroidY - web.foregroundCentroidY
    )
    let edgeRatio = max(native.edgeDensity, web.edgeDensity)
        / max(0.000_001, min(native.edgeDensity, web.edgeDensity))
    let passed = native.width == 420
        && native.height == 780
        && web.width == 420
        && web.height == 780
        && (0.01...0.75).contains(native.foregroundDensity)
        && (0.01...0.75).contains(web.foregroundDensity)
        && rowTransportDistance <= 0.16
        && centroidDelta <= 0.22
        && edgeRatio <= 4.0
    let comparison = Comparison(
        native: native,
        web: web,
        rowCorrelation: rowCorrelation,
        rowTransportDistance: rowTransportDistance,
        centroidDelta: centroidDelta,
        edgeDensityRatio: edgeRatio,
        passed: passed
    )
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
    FileHandle.standardOutput.write(try encoder.encode(comparison))
    FileHandle.standardOutput.write(Data([0x0A]))
    if !passed { exit(1) }
}

do {
    try run()
} catch {
    FileHandle.standardError.write(Data("phase4 visual comparison: \(error)\n".utf8))
    exit(1)
}
