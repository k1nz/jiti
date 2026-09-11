const baseURL = (process.env.NUXT_APP_BASE_URL || '/').replace(/\/?$/, '/')

export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  modules: ['@nuxt/fonts', '@nuxtjs/i18n'],
  css: ['~/assets/css/main.css'],
  fonts: {
    families: [
      { name: 'Baloo 2', provider: 'bunny', weights: ['600', '700', '800'] },
    ],
  },
  i18n: {
    defaultLocale: 'zh',
    strategy: 'prefix',
    locales: [
      { code: 'zh', language: 'zh-CN', name: '中文', file: 'zh-CN.json' },
      { code: 'en', language: 'en-US', name: 'English', file: 'en-US.json' },
    ],
    detectBrowserLanguage: false,
  },
  app: {
    baseURL,
    head: {
      link: [
        { rel: 'icon', type: 'image/png', href: `${baseURL}favicon.png` },
        { rel: 'apple-touch-icon', href: `${baseURL}icon.png` },
      ],
      meta: [{ name: 'theme-color', content: '#FFF4DD' }],
    },
  },
  routeRules: {
    '/': { prerender: true },
    '/zh': { prerender: true },
    '/en': { prerender: true },
  },
  nitro: {
    prerender: {
      crawlLinks: true,
      routes: ['/', '/zh', '/en'],
    },
  },
})
