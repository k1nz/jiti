<script setup lang="ts">
import { PhCaretDown, PhNotebook, PhTextAa, PhTranslate } from '@phosphor-icons/vue'

const { t } = useI18n()
const { repoUrl } = useDownloads()

useSeoMeta({
  title: () => t('meta.title'),
  description: () => t('meta.description'),
  ogTitle: () => t('meta.title'),
  ogDescription: () => t('meta.description'),
  ogImage: '/icon.png',
  themeColor: '#FFF4DD',
})

useHead({
  script: [
    {
      type: 'application/ld+json',
      innerHTML: JSON.stringify({
        '@context': 'https://schema.org',
        '@type': 'SoftwareApplication',
        name: 'Jiti',
        applicationCategory: 'UtilitiesApplication',
        operatingSystem: 'macOS, Windows',
        offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
      }),
    },
  ],
})

const features = [
  { tone: 'sun', icon: PhTranslate, title: 'feat.t1', body: 'feat.d1' },
  { tone: 'rose', icon: PhTextAa, title: 'feat.t2', body: 'feat.d2' },
  { tone: 'sky', icon: PhNotebook, title: 'feat.t3', body: 'feat.d3' },
] as const

const steps = [
  { title: 'how.s1t', body: 'how.s1d' },
  { title: 'how.s2t', body: 'how.s2d' },
  { title: 'how.s3t', body: 'how.s3d' },
] as const

const faqs = [
  { q: 'faq.q1', a: 'faq.a1' },
  { q: 'faq.q2', a: 'faq.a2' },
  { q: 'faq.q3', a: 'faq.a3' },
  { q: 'faq.q4', a: 'faq.a4' },
] as const

const macKeys = [
  { label: 'tabs.translate', combo: '⌥ ⌘ T' },
  { label: 'tabs.grammar', combo: '⌥ ⌘ G' },
  { label: 'keys.panel', combo: '⌥ ⌘ Space' },
  { label: 'keys.clip', combo: '⌥ ⌘ S' },
] as const

const winKeys = [
  { label: 'tabs.translate', combo: 'Ctrl+Shift+T' },
  { label: 'tabs.grammar', combo: 'Ctrl+Alt+G' },
  { label: 'keys.panel', combo: 'Ctrl+Alt+Space' },
  { label: 'keys.clip', combo: 'Ctrl+Alt+S' },
] as const
</script>

<template>
  <div class="page">
    <SiteHeader />

    <main>
      <section class="hero">
        <h1>{{ t('hero.title') }}</h1>
        <p class="lead">{{ t('hero.lead') }}</p>
        <DownloadButtons show-all />
      </section>

      <OverlayMock />

      <section class="features" :aria-label="t('feat.region')">
        <article v-for="item in features" :key="item.title" class="tile" :class="item.tone">
          <component :is="item.icon" :size="22" weight="regular" aria-hidden="true" />
          <h2>{{ t(item.title) }}</h2>
          <p>{{ t(item.body) }}</p>
        </article>
      </section>

      <section class="steps">
        <h2>{{ t('how.title') }}</h2>
        <p>{{ t('how.lead') }}</p>
        <div v-for="(step, index) in steps" :key="step.title" class="step">
          <span class="num">{{ index + 1 }}</span>
          <div>
            <h3>{{ t(step.title) }}</h3>
            <p>{{ t(step.body) }}</p>
          </div>
        </div>
      </section>

      <section class="keys">
        <h2>{{ t('keys.title') }}</h2>
        <div class="key-grid">
          <article class="key-card">
            <h3>macOS</h3>
            <div v-for="row in macKeys" :key="row.combo" class="key-row">
              <span>{{ t(row.label) }}</span>
              <kbd class="combo">{{ row.combo }}</kbd>
            </div>
          </article>
          <article class="key-card">
            <h3>Windows</h3>
            <div v-for="row in winKeys" :key="row.combo" class="key-row">
              <span>{{ t(row.label) }}</span>
              <kbd class="combo">{{ row.combo }}</kbd>
            </div>
          </article>
        </div>
      </section>

      <section class="faq">
        <h2>{{ t('faq.title') }}</h2>
        <details v-for="item in faqs" :key="item.q">
          <summary>
            <span>{{ t(item.q) }}</span>
            <PhCaretDown :size="16" weight="bold" aria-hidden="true" />
          </summary>
          <p>{{ t(item.a) }}</p>
        </details>
      </section>
    </main>

    <footer class="foot">
      <h2>{{ t('foot.title') }}</h2>
      <p class="lead">{{ t('foot.lead') }}</p>
      <DownloadButtons />
      <p class="legal">
        <span>{{ t('foot.copy') }}</span>
        <a :href="repoUrl"> GitHub</a>
      </p>
    </footer>
  </div>
</template>
