export const RELEASES_URL = 'https://github.com/k1nz/jiti/releases'
export const REPO_URL = 'https://github.com/k1nz/jiti'
export const FALLBACK_VERSION = 'v0.5.0-rc.4'

interface GithubAsset {
  name: string
  browser_download_url: string
}

interface GithubRelease {
  tag_name?: string
  assets?: GithubAsset[]
}

function pickAsset(assets: GithubAsset[], tests: RegExp[]) {
  return assets.find(asset => tests.every(re => re.test(asset.name)))
}

export function useDownloads() {
  const version = useState('jiti-version', () => FALLBACK_VERSION)
  const macUrl = useState('jiti-mac-url', () => RELEASES_URL)
  const winUrl = useState('jiti-win-url', () => RELEASES_URL)
  const platform = useState<'mac' | 'win' | null>('jiti-platform', () => null)

  const { data } = useLazyFetch<GithubRelease[]>('https://api.github.com/repos/k1nz/jiti/releases', {
    key: 'jiti-releases',
    server: false,
  })

  watchEffect(() => {
    const release = Array.isArray(data.value) ? data.value[0] : null
    if (!release) return
    if (release.tag_name) {
      version.value = release.tag_name.startsWith('v') ? release.tag_name : `v${release.tag_name}`
    }
    const assets = release.assets ?? []
    const mac =
      pickAsset(assets, [/\.dmg$/i, /aarch64|arm64/i])
      || pickAsset(assets, [/\.dmg$/i])
      || pickAsset(assets, [/darwin|macos/i])
    const win = pickAsset(assets, [/\.msi$/i]) || pickAsset(assets, [/\.exe$/i])
    if (mac) macUrl.value = mac.browser_download_url
    if (win) winUrl.value = win.browser_download_url
  })

  onMounted(() => {
    const ua = navigator.userAgent
    platform.value = /Mac|iPhone|iPad/.test(ua) ? 'mac' : /Win/.test(ua) ? 'win' : null
  })

  return { version, macUrl, winUrl, platform, releasesUrl: RELEASES_URL, repoUrl: REPO_URL }
}
