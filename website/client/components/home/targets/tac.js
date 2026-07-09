// @ts-check

export default class {
  targets = [
    {
      name: 'Web',
      environment: 'browser',
      description: 'Static-ready browser output with PWA assets.',
      command: 'ty bundle --target web\n# writes dist/web',
    },
    {
      name: 'macOS',
      environment: 'desktop',
      description: 'Swift WKWebView host with bundled Tac assets.',
      command: 'ty bundle --target macos\nty preview --target macos',
    },
    {
      name: 'Windows',
      environment: 'desktop',
      description: 'WebView2 desktop host for Windows machines.',
      command: 'ty bundle --target windows\nty preview --target windows',
    },
    {
      name: 'Linux',
      environment: 'desktop',
      description: 'GTK/WebKitGTK host for Linux desktops.',
      command: 'ty bundle --target linux\nty preview --target linux',
    },
    {
      name: 'iOS',
      environment: 'mobile',
      description: 'Xcode-ready iOS WKWebView project.',
      command: 'ty bundle --target ios\nty preview --target ios',
    },
    {
      name: 'Android',
      environment: 'mobile',
      description: 'Android Studio-ready WebView project.',
      command: 'ty bundle --target android\nty preview --target android',
    },
  ]

  allTargetsCommand = [
    'ty bundle --target all',
    'ty preview --target web',
    'ty preview --target macos',
  ].join('\n')
}
