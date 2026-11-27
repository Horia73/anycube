#import "NativeCamera.h"
#import <AVFoundation/AVFoundation.h>
#import <CoreImage/CoreImage.h>

// Continuity cameras can still be reported as external on some Macs.
static NSArray<AVCaptureDevice *> *phones(void) {
    NSMutableArray *types = [NSMutableArray arrayWithObject:AVCaptureDeviceTypeExternalUnknown];
#if __MAC_OS_X_VERSION_MAX_ALLOWED >= 140000
    if (@available(macOS 14.0, *)) {
        types = [NSMutableArray arrayWithObjects:AVCaptureDeviceTypeExternal, AVCaptureDeviceTypeContinuityCamera, nil];
    }
#endif
    NSArray *devices = [AVCaptureDeviceDiscoverySession discoverySessionWithDeviceTypes:types
        mediaType:AVMediaTypeVideo position:AVCaptureDevicePositionUnspecified].devices;
    NSMutableArray *found = [NSMutableArray array];
    for (AVCaptureDevice *device in devices) {
        NSString *name = device.localizedName.lowercaseString;
        if (!device.connected || [name containsString:@"desk view"] || [name containsString:@"deskview"]
            || [name containsString:@"facetime"] || [name containsString:@"front"]) continue;
        BOOL phone = [name containsString:@"iphone"];
#if __MAC_OS_X_VERSION_MAX_ALLOWED >= 140000
        if (@available(macOS 14.0, *)) phone |= [device.deviceType isEqual:AVCaptureDeviceTypeContinuityCamera];
#endif
        if (phone) [found addObject:device];
    }
    return found;
}

@interface NativeCamera () <AVCaptureVideoDataOutputSampleBufferDelegate>
@end

@implementation NativeCamera {
    dispatch_queue_t _queue;
    NSString *_token;
    AVCaptureSession *_session;
    AVCaptureVideoDataOutput *_output;
    CIImage *_latest;
    CIContext *_context;
}

- (instancetype)init {
    if ((self = [super init])) _queue = dispatch_queue_create("local.anycube.camera", DISPATCH_QUEUE_SERIAL);
    return self;
}

// All session state and sample callbacks live on the same serial queue.
- (void)stop {
    _token = nil;
    [_output setSampleBufferDelegate:nil queue:NULL];
    [_session stopRunning];
    _output = nil;
    _session = nil;
    _latest = nil;
}

- (void)reset {
    dispatch_async(_queue, ^{ [self stop]; });
}

- (void)shutdown {
    dispatch_sync(_queue, ^{ [self stop]; });
}

- (void)captureOutput:(AVCaptureOutput *)output didOutputSampleBuffer:(CMSampleBufferRef)buffer
       fromConnection:(AVCaptureConnection *)connection {
    (void)connection;
    CVImageBufferRef image = CMSampleBufferGetImageBuffer(buffer);
    if (image && output == _output) _latest = [CIImage imageWithCVPixelBuffer:image];
}

- (void)startDevice:(NSString *)identifier reply:(CameraReply)reply {
    AVCaptureDevice *selected = nil;
    for (AVCaptureDevice *device in phones()) {
        if (!identifier.length || [device.uniqueID isEqual:identifier]) { selected = device; break; }
    }
    if (!selected) { reply(nil, @"iPhone camera unavailable"); return; }
    NSError *error = nil;
    AVCaptureDeviceInput *input = [AVCaptureDeviceInput deviceInputWithDevice:selected error:&error];
    AVCaptureSession *session = [AVCaptureSession new];
    AVCaptureVideoDataOutput *output = [AVCaptureVideoDataOutput new];
    output.alwaysDiscardsLateVideoFrames = YES;
    output.videoSettings = @{ (__bridge NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA) };
    if (!input || ![session canAddInput:input]) {
        reply(nil, error.localizedDescription ?: @"Cannot open iPhone camera"); return;
    }
    [session addInput:input];
    if (![session canAddOutput:output]) { reply(nil, @"Cannot read iPhone camera frames"); return; }
    [session addOutput:output];
    if ([session canSetSessionPreset:AVCaptureSessionPreset1280x720]) session.sessionPreset = AVCaptureSessionPreset1280x720;
    [output setSampleBufferDelegate:self queue:_queue];
    _session = session;
    _output = output;
    [session startRunning];
    if (!session.running) { [self stop]; reply(nil, @"Cannot start iPhone camera"); return; }
    reply(@{@"id":selected.uniqueID, @"label":selected.localizedName}, nil);
}

- (void)handle:(NSDictionary *)request reply:(CameraReply)handler {
    // WebKit reply handlers must be called on the main thread.
    CameraReply reply = ^(id value, NSString *error) {
        dispatch_async(dispatch_get_main_queue(), ^{ handler(value, error); });
    };
    dispatch_async(_queue, ^{
        NSString *op = request[@"op"];
        if ([op isEqual:@"list"]) {
            NSMutableArray *list = [NSMutableArray array];
            for (AVCaptureDevice *d in phones()) [list addObject:@{@"id":d.uniqueID, @"label":d.localizedName}];
            reply(list, nil);
            return;
        }
        NSString *token = request[@"token"];
        if (![token isKindOfClass:NSString.class] || !token.length || token.length > 128) {
            reply(nil, @"Invalid camera session"); return;
        }
        if ([op isEqual:@"start"]) {
            NSString *device = request[@"id"];
            if (device && ![device isKindOfClass:NSString.class]) { reply(nil, @"Invalid camera ID"); return; }
            [self stop];
            self->_token = [token copy];
            [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL allowed) {
                dispatch_async(self->_queue, ^{
                    if (![self->_token isEqual:token]) { reply(nil, @"Camera choice changed"); return; }
                    if (!allowed) { [self stop]; reply(nil, @"Camera permission denied"); return; }
                    [self startDevice:device reply:reply];
                });
            }];
        } else if (![self->_token isEqual:token]) {
            reply(nil, @"Camera session ended");
        } else if ([op isEqual:@"stop"]) {
            [self stop];
            reply(@YES, nil);
        } else if ([op isEqual:@"frame"]) {
            CIImage *image = self->_latest;
            self->_latest = nil; // A second pull must wait for a new sample.
            if (!image) { reply(NSNull.null, nil); return; }
            if (!self->_context) self->_context = [CIContext contextWithOptions:nil];
            CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
            NSData *jpeg = [self->_context JPEGRepresentationOfImage:image colorSpace:space options:@{}];
            CGColorSpaceRelease(space);
            if (!jpeg) { reply(nil, @"Cannot encode iPhone frame"); return; }
            reply([jpeg base64EncodedStringWithOptions:0], nil);
        } else {
            reply(nil, @"Unknown camera request");
        }
    });
}
@end
