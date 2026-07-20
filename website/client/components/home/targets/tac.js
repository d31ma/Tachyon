// @ts-check

export default class {
  targets = [
    {
      name: 'Web',
      platform: 'web platform',
      description: 'Static-ready browser output with PWA assets.',
      command: 'ty bundle --target web\n# writes dist/web',
    },
    {
      name: 'macOS',
      platform: 'desktop platform',
      description: 'SwiftUI controls driven by Tac\'s DOM-free controller.',
      command: 'ty bundle --target macos\nty preview --target macos',
    },
    {
      name: 'Windows',
      platform: 'desktop platform',
      description: 'WinUI controls with an embedded QuickJS controller.',
      command: 'ty bundle --target windows\nty preview --target windows',
    },
    {
      name: 'Linux',
      platform: 'desktop platform',
      description: 'GTK controls with an embedded QuickJS controller.',
      command: 'ty bundle --target linux\nty preview --target linux',
    },
    {
      name: 'iOS',
      platform: 'mobile platform',
      description: 'Xcode-ready SwiftUI with local fallback for unsupported subtrees.',
      command: 'ty bundle --target ios\nty preview --target ios',
    },
    {
      name: 'Android',
      platform: 'mobile platform',
      description: 'Jetpack Compose project with an embedded controller.',
      command: 'ty bundle --target android\nty preview --target android',
    },
  ]

  allTargetsCommand = [
    'ty bundle --target all',
    'ty bundle --target android',
    'ty preview --target web',
    'ty preview --target macos',
  ].join('\n')
}
