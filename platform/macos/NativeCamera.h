#import <Foundation/Foundation.h>

typedef void (^CameraReply)(id, NSString *);

// The host authenticates the sending frame before forwarding a request here.
@interface NativeCamera : NSObject
- (void)handle:(NSDictionary *)request reply:(CameraReply)reply;
- (void)reset;
- (void)shutdown;
@end
