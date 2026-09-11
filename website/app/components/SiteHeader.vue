<script setup lang="ts">
import { PhGithubLogo } from '@phosphor-icons/vue'

const { t, locale } = useI18n()
const localePath = useLocalePath()
const switchLocalePath = useSwitchLocalePath()
const { version, repoUrl } = useDownloads()
const iconSrc = `${useRuntimeConfig().app.baseURL}icon.png`
</script>

<template>
  <header class="top">
    <NuxtLink class="brand" :to="localePath('/')">
      <img :src="iconSrc" width="44" height="44" alt="" />
      <span class="brand-name">Jiti</span>
      <span class="ver">{{ version }}</span>
    </NuxtLink>
    <div class="top-actions">
      <div class="lang" role="group" :aria-label="t('lang')">
        <NuxtLink
          :to="switchLocalePath('zh')"
          :aria-pressed="locale === 'zh'"
        >
          中
        </NuxtLink>
        <NuxtLink
          :to="switchLocalePath('en')"
          :aria-pressed="locale === 'en'"
        >
          EN
        </NuxtLink>
      </div>
      <a class="icon-btn" :href="repoUrl" :aria-label="t('github')">
        <PhGithubLogo :size="18" weight="regular" aria-hidden="true" />
      </a>
    </div>
  </header>
</template>
