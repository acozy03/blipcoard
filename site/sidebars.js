// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docsSidebar: [
    {
      type: 'category',
      label: 'Start',
      items: ['start/overview', 'start/setup', 'start/product-model'],
    },
    {
      type: 'category',
      label: 'Product',
      items: [
        'operations/cli-operations',
        'operations/desktop-bundles',
        'operations/runtime-distribution',
      ],
    },
    {
      type: 'category',
      label: 'Architecture',
      items: [
        'architecture/runtime-boundary',
        'architecture/runtime-model',
        'architecture/privacy-and-policy',
        'architecture/hosted-workspaces',
        'architecture/hosted-threat-model',
        'architecture/hosted-sync-protocol',
        'architecture/storage-and-blobs',
        {
          type: 'category',
          label: 'ADRs',
          items: ['architecture/adr/typed-clipboard-payloads'],
        },
      ],
    },
    {
      type: 'category',
      label: 'Operations',
      items: [
        'operations/upgrade-migrations',
        'operations/cli-operations',
        'operations/desktop-bundles',
        'operations/hosted-deployment',
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: ['reference/cli', 'reference/daemon-api', 'reference/troubleshooting'],
    },
    {
      type: 'category',
      label: 'Contributor',
      items: [
        'contributor/development-workflow',
        'contributor/docs-maintenance',
        'contributor/repository-layout',
        'contributor/docs-site-architecture',
      ],
    },
    {
      type: 'category',
      label: 'Roadmap',
      items: ['roadmap/phase-model', 'roadmap/mvp-phases', 'roadmap/project-breakdown'],
    },
  ],
};

export default sidebars;
