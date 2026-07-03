// @ts-check
import {themes as prismThemes} from 'prism-react-renderer';
import npm2yarn from '@docusaurus/remark-plugin-npm2yarn';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'blipcoard',
  tagline: 'Runtime-first clipboard routing for agent workflows',
  favicon: 'img/blipcoard-removebg-preview.png',

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
          remarkPlugins: [[npm2yarn, {sync: true, converters: ['yarn', 'pnpm']}]],
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  plugins: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexDocs: true,
        indexPages: true,
        indexBlog: false,
        language: ['en'],
        docsRouteBasePath: '/docs',
      },
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: 'img/blipcoard-removebg-preview.png',
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        logo: {
          alt: 'blipcoard logo',
          src: 'img/blipcoard-removebg-preview.png',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Docs',
          },
          {type: 'doc', docId: 'start/setup', label: 'Setup', position: 'left'},
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
              {label: 'One-command setup', to: '/docs/start/setup'},
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
              {label: 'Hosted sync protocol', to: '/docs/architecture/hosted-sync-protocol'},
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
        darkTheme: prismThemes.vsDark,
        additionalLanguages: ['bash', 'diff', 'powershell', 'rust', 'toml'],
        magicComments: [
          {
            className: 'theme-code-block-highlighted-line',
            line: 'highlight-next-line',
            block: {start: 'highlight-start', end: 'highlight-end'},
          },
          {
            className: 'code-block-error-line',
            line: 'error-next-line',
            block: {start: 'error-start', end: 'error-end'},
          },
          {
            className: 'code-block-success-line',
            line: 'success-next-line',
            block: {start: 'success-start', end: 'success-end'},
          },
        ],
      },
    }),
};

export default config;
