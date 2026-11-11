// The interface in a window of its own, on macOS, with what the system
// already has: this is JavaScript for Automation (`osascript -l JavaScript`)
// driving a Cocoa window with a WebKit view in it. No browser, no tabs.
//
//   osascript -l JavaScript window.js URL [ICON.png]
//   osascript -l JavaScript window.js --serve /path/to/anycube [ICON.png]
//
// With --serve it starts the local server itself (`anycube app
// --no-window --with-parent`), reads the address the server prints, and stops the server
// when the window closes: that is how the entry in Applications runs.

ObjC.import("Cocoa");
ObjC.import("WebKit");

function run(argv) {
  let url = argv[0];
  let icon = argv[1];
  let server = null;
  if (argv[0] === "--serve") {
    icon = argv[2];
    server = $.NSTask.alloc.init;
    server.executableURL = $.NSURL.fileURLWithPath(argv[1]);
    server.arguments = ["app", "--no-window", "--with-parent"];
    const out = $.NSPipe.pipe;
    server.standardOutput = out;
    if (!server.launchAndReturnError(null)) throw new Error("could not start " + argv[1]);
    // The first line the server prints is its address.
    const first = out.fileHandleForReading.availableData;
    url = ObjC.unwrap($.NSString.alloc.initWithDataEncoding(first, $.NSUTF8StringEncoding)).split("\n")[0].trim();
    if (!/^http:\/\/127\.0\.0\.1:\d+\/$/.test(url)) throw new Error("the server did not say where it is");
  }

  const app = $.NSApplication.sharedApplication;
  app.setActivationPolicy($.NSApplicationActivationPolicyRegular);
  if (icon) {
    const image = $.NSImage.alloc.initWithContentsOfFile(icon);
    if (!image.isNil()) app.setApplicationIconImage(image);
  }

  // A menu, so that Quit, Close and the clipboard keys work.
  const item = (title, action, key) => $.NSMenuItem.alloc.initWithTitleActionKeyEquivalent(title, action, key);
  const bar = $.NSMenu.alloc.initWithTitle("");
  const appItem = item("", null, "");
  const appMenu = $.NSMenu.alloc.initWithTitle("anycube");
  appMenu.addItem(item("Hide anycube", "hide:", "h"));
  appMenu.addItem($.NSMenuItem.separatorItem);
  appMenu.addItem(item("Quit anycube", "terminate:", "q"));
  appItem.submenu = appMenu;
  bar.addItem(appItem);
  const editItem = item("Edit", null, "");
  const editMenu = $.NSMenu.alloc.initWithTitle("Edit");
  editMenu.addItem(item("Undo", "undo:", "z"));
  editMenu.addItem(item("Redo", "redo:", "Z"));
  editMenu.addItem($.NSMenuItem.separatorItem);
  editMenu.addItem(item("Cut", "cut:", "x"));
  editMenu.addItem(item("Copy", "copy:", "c"));
  editMenu.addItem(item("Paste", "paste:", "v"));
  editMenu.addItem(item("Select All", "selectAll:", "a"));
  editItem.submenu = editMenu;
  bar.addItem(editItem);
  const windowItem = item("Window", null, "");
  const windowMenu = $.NSMenu.alloc.initWithTitle("Window");
  windowMenu.addItem(item("Minimize", "performMiniaturize:", "m"));
  windowMenu.addItem(item("Close", "performClose:", "w"));
  windowItem.submenu = windowMenu;
  bar.addItem(windowItem);
  app.mainMenu = bar;

  const style =
    $.NSWindowStyleMaskTitled | $.NSWindowStyleMaskClosable | $.NSWindowStyleMaskMiniaturizable | $.NSWindowStyleMaskResizable;
  const frame = $.NSMakeRect(0, 0, 1280, 820);
  const win = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer(frame, style, $.NSBackingStoreBuffered, false);
  win.title = "anycube";
  win.minSize = $.NSMakeSize(360, 520);
  win.center;

  // The page keeps nothing on disk: its data lives for as long as the window does.
  const conf = $.WKWebViewConfiguration.alloc.init;
  conf.websiteDataStore = $.WKWebsiteDataStore.nonPersistentDataStore;
  const web = $.WKWebView.alloc.initWithFrameConfiguration(win.contentView.bounds, conf);
  web.autoresizingMask = $.NSViewWidthSizable | $.NSViewHeightSizable;
  win.contentView.addSubview(web);
  web.loadRequest($.NSURLRequest.requestWithURL($.NSURL.URLWithString(url)));

  // Closing the window ends the application, and the server with it.
  ObjC.registerSubclass({
    name: "AnycubeWindowDelegate",
    protocols: ["NSWindowDelegate"],
    methods: {
      "windowWillClose:": {
        types: ["void", ["id"]],
        implementation: function () {
          if (server) server.terminate;
          $.NSApplication.sharedApplication.terminate(null);
        },
      },
    },
  });
  const delegate = $.AnycubeWindowDelegate.alloc.init;
  win.delegate = delegate;

  win.makeKeyAndOrderFront(null);
  // Comes to the front, as an application that was just opened does; a
  // test that opens it behind someone at work sets ANYCUBE_BEHIND.
  const behind = $.NSProcessInfo.processInfo.environment.objectForKey("ANYCUBE_BEHIND");
  if (behind.isNil()) app.activateIgnoringOtherApps(true);
  else win.orderBack(null);
  app.run;
  if (server) server.terminate;
}
