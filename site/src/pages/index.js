import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import styles from './index.module.css';

const sections = [
  {
    title: 'Install and Run',
    text: 'Set up the daemon-first runtime, CLI-only mode, desktop bundles, and upgrades.',
    to: '/docs/operations/cli-operations',
  },
  {
    title: 'Architecture',
    text: 'Understand blipd, the daemon API, local SQLite storage, and rich payload blobs.',
    to: '/docs/architecture/runtime-model',
  },
  {
    title: 'Reference',
    text: 'Use daemon API and operational reference docs while building or debugging.',
    to: '/docs/reference/daemon-api',
  },
  {
    title: 'Roadmap',
    text: 'Follow the MVP phases and implementation breakdown that guide the project.',
    to: '/docs/roadmap/mvp-phases',
  },
];

export default function Home() {
  return (
    <Layout
      title="blipcoard docs"
      description="Documentation for blipcoard clipboard routing">
      <main className={styles.main}>
        <section className={styles.hero}>
          <p className={styles.kicker}>blipcoard documentation</p>
          <h1>Runtime-first clipboard routing for agent workflows.</h1>
          <p className={styles.lede}>
            Learn how blipd owns clipboard ingestion, how clients route blips
            through workspaces, and how local storage stays auditable.
          </p>
          <div className={styles.actions}>
            <Link className="button button--primary" to="/docs/start/overview">
              Read the docs
            </Link>
            <Link className="button button--secondary" to="/docs/operations/cli-operations">
              CLI operations
            </Link>
          </div>
        </section>
        <section className={styles.grid} aria-label="Documentation sections">
          {sections.map((section) => (
            <Link className={styles.card} to={section.to} key={section.title}>
              <h2>{section.title}</h2>
              <p>{section.text}</p>
            </Link>
          ))}
        </section>
      </main>
    </Layout>
  );
}
