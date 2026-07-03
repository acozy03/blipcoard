// @ts-check
import {themes as prismThemes} from 'prism-react-renderer';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'blipcoard',
  tagline: 'Runtime-first clipboard routing for agent workflows',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://blipcoard.github.io',
  baseUrl: '/blipcoard/',
  organizationName: 'blipcoard',
  projectName: 'blipcoard',

  onBrokenLinks: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: 'docs',
          sidebarPath: './sidebars.js',
          editUrl: 'https://github.com/blipcoard/blipcoard/tree/develop/site/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: 'img/logo.svg',
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'blipcoard',
        logo: {
          alt: 'blipcoard logo',
          src: 'img/logo.svg',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Docs',
          },
          {type: 'doc', docId: 'start/product-model', label: 'Product', position: 'left'},
          {
            type: 'doc',
            docId: 'architecture/runtime-model',
            label: 'Architecture',
            position: 'left',
          },
          {
            type: 'doc',
            docId: 'reference/cli',
            label: 'Reference',
            position: 'left',
          },
          {
            type: 'doc',
            docId: 'contributor/development-workflow',
            label: 'Contribute',
            position: 'left',
          },
          {
            href: 'https://github.com/blipcoard/blipcoard',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              {label: 'Overview', to: '/docs/start/overview'},
              {label: 'Product model', to: '/docs/start/product-model'},
              {label: 'CLI operations', to: '/docs/operations/cli-operations'},
              {label: 'Troubleshooting', to: '/docs/reference/troubleshooting'},
              {label: 'Upgrade and migrations', to: '/docs/operations/upgrade-migrations'},
            ],
          },
          {
            title: 'Reference',
            items: [
              {label: 'Runtime boundary', to: '/docs/architecture/runtime-boundary'},
              {label: 'Hosted workspaces', to: '/docs/architecture/hosted-workspaces'},
              {label: 'Hosted threat model', to: '/docs/architecture/hosted-threat-model'},
              {label: 'Architecture', to: '/docs/architecture/runtime-model'},
              {label: 'CLI reference', to: '/docs/reference/cli'},
              {label: 'Daemon API', to: '/docs/reference/daemon-api'},
              {label: 'Roadmap', to: '/docs/roadmap/mvp-phases'},
            ],
          },
          {
            title: 'Project',
            items: [
              {label: 'GitHub', href: 'https://github.com/blipcoard/blipcoard'},
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} blipcoard contributors.`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
      },
    }),
};

export default config;
