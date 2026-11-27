#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>

@interface AnycubeHost : NSObject <NSApplicationDelegate, WKUIDelegate, WKNavigationDelegate>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) WKWebView *webView;
@property(nonatomic, strong) NSTask *server;
@property(nonatomic, strong) NSURL *address;
@property(nonatomic) BOOL stopping;
@end

@implementation AnycubeHost

- (void)fail:(NSString *)detail {
    if (self.stopping) return;
    self.stopping = YES;
    NSAlert *alert = [NSAlert new];
    alert.alertStyle = NSAlertStyleCritical;
    alert.messageText = @"anycube could not open";
    alert.informativeText = detail;
    [NSApp activateIgnoringOtherApps:YES];
    [alert runModal];
    [NSApp terminate:nil];
}

- (BOOL)matches:(NSString *)scheme host:(NSString *)host port:(NSInteger)port {
    return self.address && [scheme isEqualToString:@"http"]
        && [host isEqualToString:@"127.0.0.1"] && port == self.address.port.integerValue;
}

- (BOOL)allowsURL:(NSURL *)url {
    return !url.user && !url.password && [self matches:url.scheme host:url.host port:url.port.integerValue];
}

- (void)webView:(WKWebView *)webView decidePolicyForNavigationAction:(WKNavigationAction *)action
decisionHandler:(void (^)(WKNavigationActionPolicy))decision {
    if (webView == self.webView && !action.targetFrame) {
        [self handleReleaseLink:action];
        decision(WKNavigationActionPolicyCancel);
        return;
    }
    if (webView == self.webView && action.targetFrame.isMainFrame && [self allowsURL:action.request.URL]) {
        decision(WKNavigationActionPolicyAllow);
        return;
    }
    decision(WKNavigationActionPolicyCancel);
    if (action.targetFrame && webView == self.webView) [self handleReleaseLink:action];
}

- (void)webView:(WKWebView *)webView decidePolicyForNavigationResponse:(WKNavigationResponse *)response
decisionHandler:(void (^)(WKNavigationResponsePolicy))decision {
    decision(webView == self.webView && response.isForMainFrame && [self allowsURL:response.response.URL]
        ? WKNavigationResponsePolicyAllow : WKNavigationResponsePolicyCancel);
}

- (WKWebView *)webView:(WKWebView *)webView createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration
   forNavigationAction:(WKNavigationAction *)action windowFeatures:(WKWindowFeatures *)features {
    (void)configuration;
    (void)features;
    if (webView == self.webView) [self handleReleaseLink:action];
    return nil;
}

- (void)handleReleaseLink:(WKNavigationAction *)action {
    NSString *release = @"https://github.com/Horia73/anycube/releases/latest";
    if (self.window.attachedSheet || !action.sourceFrame.isMainFrame
        || ![self allowsURL:action.sourceFrame.request.URL]
        || action.navigationType != WKNavigationTypeLinkActivated
        || ![action.request.URL.absoluteString isEqualToString:release]) return;
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"anycube releases";
    alert.informativeText = release;
    [alert addButtonWithTitle:@"Copy Link"];
    [alert addButtonWithTitle:@"Close"];
    [alert beginSheetModalForWindow:self.window completionHandler:^(NSModalResponse result) {
        if (result == NSAlertFirstButtonReturn) {
            [NSPasteboard.generalPasteboard clearContents];
            [NSPasteboard.generalPasteboard setString:release forType:NSPasteboardTypeString];
        }
    }];
}

- (void)webView:(WKWebView *)webView requestMediaCapturePermissionForOrigin:(WKSecurityOrigin *)origin
initiatedByFrame:(WKFrameInfo *)frame type:(WKMediaCaptureType)type
 decisionHandler:(void (^)(WKPermissionDecision))decision {
    BOOL allowed = webView == self.webView && frame.isMainFrame && type == WKMediaCaptureTypeCamera
        && [self allowsURL:frame.request.URL] && [self allowsURL:webView.URL]
        && [self matches:origin.protocol host:origin.host port:origin.port];
    decision(allowed ? WKPermissionDecisionPrompt : WKPermissionDecisionDeny);
}

- (void)webView:(WKWebView *)webView didFailProvisionalNavigation:(WKNavigation *)navigation
      withError:(NSError *)error {
    (void)webView;
    (void)navigation;
    if (!self.webView.URL && error.code != NSURLErrorCancelled) [self fail:error.localizedDescription];
}

- (void)webViewWebContentProcessDidTerminate:(WKWebView *)webView {
    [webView loadRequest:[NSURLRequest requestWithURL:self.address]];
}

- (void)focus {
    if (self.window.miniaturized) [self.window deminiaturize:nil];
    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
}

- (void)installMenu {
    NSMenu *bar = [NSMenu new];
    NSMenuItem *app = [bar addItemWithTitle:@"anycube" action:NULL keyEquivalent:@""];
    app.submenu = [[NSMenu alloc] initWithTitle:@"anycube"];
    [app.submenu addItemWithTitle:@"Hide anycube" action:@selector(hide:) keyEquivalent:@"h"];
    [app.submenu addItemWithTitle:@"Quit anycube" action:@selector(terminate:) keyEquivalent:@"q"];
    NSMenuItem *edit = [bar addItemWithTitle:@"Edit" action:NULL keyEquivalent:@""];
    edit.submenu = [[NSMenu alloc] initWithTitle:@"Edit"];
    [edit.submenu addItemWithTitle:@"Undo" action:@selector(undo:) keyEquivalent:@"z"];
    [edit.submenu addItemWithTitle:@"Redo" action:@selector(redo:) keyEquivalent:@"Z"];
    [edit.submenu addItemWithTitle:@"Cut" action:@selector(cut:) keyEquivalent:@"x"];
    [edit.submenu addItemWithTitle:@"Copy" action:@selector(copy:) keyEquivalent:@"c"];
    [edit.submenu addItemWithTitle:@"Paste" action:@selector(paste:) keyEquivalent:@"v"];
    [edit.submenu addItemWithTitle:@"Select All" action:@selector(selectAll:) keyEquivalent:@"a"];
    NSMenuItem *window = [bar addItemWithTitle:@"Window" action:NULL keyEquivalent:@""];
    window.submenu = [[NSMenu alloc] initWithTitle:@"Window"];
    [window.submenu addItemWithTitle:@"Minimize" action:@selector(performMiniaturize:) keyEquivalent:@"m"];
    [window.submenu addItemWithTitle:@"Close" action:@selector(performClose:) keyEquivalent:@"w"];
    NSApp.mainMenu = bar;
    NSApp.windowsMenu = window.submenu;
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    [self installMenu];
    self.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 1280, 820)
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered defer:NO];
    self.window.releasedWhenClosed = NO;
    self.window.title = @"anycube";
    self.window.minSize = NSMakeSize(360, 520);
    [self.window center];
    [self focus];

    NSTask *server = [NSTask new];
    server.executableURL = [NSBundle.mainBundle.executableURL.URLByDeletingLastPathComponent URLByAppendingPathComponent:@"anycube-server"];
    server.arguments = @[@"app", @"--no-window", @"--with-parent", @"--desktop-session"];
    NSPipe *pipe = [NSPipe pipe];
    server.standardOutput = pipe;
    __weak AnycubeHost *host = self;
    server.terminationHandler = ^(NSTask *task) {
        dispatch_async(dispatch_get_main_queue(), ^{
            if (task.terminationStatus == 0) { [NSApp terminate:nil]; return; }
            [host fail:[NSString stringWithFormat:@"The local server exited (status %d).", task.terminationStatus]];
        });
    };
    NSError *error;
    if (![server launchAndReturnError:&error]) {
        [self fail:error.localizedDescription];
        return;
    }
    self.server = server;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        if (!self.address) [self fail:@"The local server did not report its address."];
    });
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        NSFileHandle *output = pipe.fileHandleForReading;
        NSMutableData *line = [NSMutableData new];
        while (line.length < 160) {
            NSData *byte = [output readDataUpToLength:1 error:NULL];
            if (!byte.length || *(const char *)byte.bytes == '\n') break;
            [line appendData:byte];
        }
        [output closeFile];
        NSString *address = [[NSString alloc] initWithData:line encoding:NSUTF8StringEncoding];
        dispatch_async(dispatch_get_main_queue(), ^{
            if (self.stopping) return;
            [self loadAddress:address];
        });
    });
}

- (void)loadAddress:(NSString *)address {
    NSURL *url = address ? [NSURL URLWithString:address] : nil;
    NSInteger port = url.port.integerValue;
    NSString *expected = [NSString stringWithFormat:@"http://127.0.0.1:%ld/?window#update=", (long)port];
    if (port < 1 || port > 65535 || ![address hasPrefix:expected] || address.length != expected.length + 64 ||
        [[address substringFromIndex:expected.length] rangeOfCharacterFromSet:
            [NSCharacterSet characterSetWithCharactersInString:@"0123456789abcdef"].invertedSet].location != NSNotFound) {
        [self fail:@"The local server reported an invalid address."];
        return;
    }
    self.address = url;
    WKWebViewConfiguration *config = [WKWebViewConfiguration new];
    [config.userContentController addUserScript:[[WKUserScript alloc]
        initWithSource:@"window.anycubeNativeMac = true"
        injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:YES]];
    config.websiteDataStore = WKWebsiteDataStore.nonPersistentDataStore;
    self.webView = [[WKWebView alloc] initWithFrame:self.window.contentView.bounds configuration:config];
    self.webView.UIDelegate = self;
    self.webView.navigationDelegate = self;
    self.window.contentView = self.webView;
    [self.webView loadRequest:[NSURLRequest requestWithURL:url]];
}

- (BOOL)applicationShouldHandleReopen:(NSApplication *)app hasVisibleWindows:(BOOL)visible {
    (void)app;
    (void)visible;
    [self focus];
    return NO;
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)app {
    (void)app;
    return YES;
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    (void)notification;
    self.stopping = YES;
    // The parent watcher allows an active update to finish before exiting.
}

@end

int main(void) {
    @autoreleasepool {
        NSApplication *app = NSApplication.sharedApplication;
        [app setActivationPolicy:NSApplicationActivationPolicyRegular];
        __attribute__((objc_precise_lifetime)) AnycubeHost *host = [AnycubeHost new];
        app.delegate = host;
        [app run];
    }
    return 0;
}
