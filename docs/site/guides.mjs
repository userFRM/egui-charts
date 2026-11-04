// Sources and edit paths are relative to the repository root, keeping one source per guide.
export const guides = [
  { slug: 'builder-guide', source: 'docs/builder-guide.md', editPath: 'docs/builder-guide.md', title: 'Builder guide' },
  { slug: 'study-data', source: 'docs/study-data.md', editPath: 'docs/study-data.md', title: 'Calendar and viewport studies', scene: 'order-flow' },
  { slug: 'order-flow', source: 'docs/order-flow.md', editPath: 'docs/order-flow.md', title: 'Order flow', scene: 'order-flow' },
  { slug: 'replay', source: 'docs/order-flow-replay.md', editPath: 'docs/order-flow-replay.md', title: 'Recording and replay', scene: 'recording-replay' },
  { slug: 'strategy-report', source: 'docs/strategy-report.md', editPath: 'docs/strategy-report.md', title: 'Strategy reports', scene: 'host-report' },
  { slug: 'host-services', source: 'docs/host-data-services.md', editPath: 'docs/host-data-services.md', title: 'Host data services', scene: 'host-report' },
  { slug: 'performance', source: 'docs/performance.md', editPath: 'docs/performance.md', title: 'Performance' },
  { slug: 'release-notes', source: 'CHANGELOG.md', editPath: 'CHANGELOG.md', title: 'Release notes' },
];
