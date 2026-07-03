// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docsSidebar: [
    {
      type: 'category',
      label: 'Start',
      items: ['start/overview'],
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
        'architecture/runtime-model',
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
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: ['reference/daemon-api'],
    },
    {
      type: 'category',
      label: 'Contributor',
      items: ['contributor/repository-layout', 'contributor/docs-site-architecture'],
    },
    {
      type: 'category',
      label: 'Roadmap',
      items: ['roadmap/mvp-phases', 'roadmap/project-breakdown'],
    },
  ],
};

export default sidebars;
