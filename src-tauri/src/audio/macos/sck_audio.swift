import Foundation
import ScreenCaptureKit
import CoreMedia
import CoreGraphics
import AVFoundation

class AudioStreamDelegate: NSObject, SCStreamOutput {
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }
        
        // Get the audio format description to know the actual format
        guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer) else { return }
        let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription)?.pointee
        
        guard let blockBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }
        
        var length = 0
        var dataPointer: UnsafeMutablePointer<Int8>? = nil
        let status = CMBlockBufferGetDataPointer(blockBuffer, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &dataPointer)
        
        guard status == kCMBlockBufferNoErr, let rawPtr = dataPointer, length > 0 else { return }
        
        // SCK delivers Float32 PCM by default
        // Convert Float32 -> Int16 before sending to Rust
        let floatPtr = UnsafeRawPointer(rawPtr).bindMemory(to: Float32.self, capacity: length / 4)
        let sampleCount = length / 4
        
        var int16Samples = [Int16](repeating: 0, count: sampleCount)
        for i in 0..<sampleCount {
            let f = max(-1.0, min(1.0, floatPtr[i]))
            int16Samples[i] = Int16(f * Float32(Int16.max))
        }
        
        int16Samples.withUnsafeBytes { ptr in
            let data = Data(ptr)
            FileHandle.standardOutput.write(data)
        }
    }
}

var activeStream: SCStream?
var activeDelegate: AudioStreamDelegate?

func startCapture() {
    let semaphore = DispatchSemaphore(value: 0)
    
    SCShareableContent.getWithCompletionHandler { content, error in
        guard let content = content else {
            fputs("Failed to get content: \(String(describing: error))\n", stderr)
            exit(1)
        }
        
        guard let display = content.displays.first else {
            fputs("No displays found\n", stderr)
            exit(1)
        }
        
        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])
        
        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.sampleRate = 48000
        config.channelCount = 1
        config.excludesCurrentProcessAudio = true
        
        activeDelegate = AudioStreamDelegate()
        activeStream = SCStream(filter: filter, configuration: config, delegate: nil)
        
        fputs("SCK: starting capture at 48kHz mono Float32->Int16\n", stderr)
        
        do {
            try activeStream?.addStreamOutput(activeDelegate!, type: .audio, sampleHandlerQueue: .global())
            activeStream?.startCapture { error in
                if let error = error {
                    fputs("Failed to start capture: \(error)\n", stderr)
                    exit(1)
                }
                fputs("SCK: capture started successfully\n", stderr)
            }
        } catch {
            fputs("Error adding stream output: \(error)\n", stderr)
            exit(1)
        }
    }
    
    // Exit if stdin is closed (parent process died)
    DispatchQueue.global().async {
        let _ = FileHandle.standardInput.readDataToEndOfFile()
        fputs("SCK: stdin closed, exiting\n", stderr)
        exit(0)
    }
    
    semaphore.wait()
}

// Request permission first
if !CGPreflightScreenCaptureAccess() {
    CGRequestScreenCaptureAccess()
    fputs("Requesting Screen Capture Access. Please grant permission and restart.\n", stderr)
    exit(2)
}

fputs("SCK: binary starting\n", stderr)
startCapture()
