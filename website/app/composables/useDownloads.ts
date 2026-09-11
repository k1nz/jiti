import {
  FALLBACK_VERSION,
  RELEASES_URL,
  REPO_URL,
  isPreRelease,
  normalizeTag,
  pickDownloadRelease,
  pickMacAsset,
  pickWinAsset,
  releasePageUrl,
  type GithubRelease,
} from '~/utils/releases'

export { FALLBACK_VERSION, RELEASES_URL, REPO_URL }

export function useDownloads() {
  const version = useState('jiti-version', () => FALLBACK_VERSION)
  const channel = useState<'stable' | 'rc'>('jiti-channel', () => 'rc')
  const macUrl = useState('jiti-mac-url', () => releasePageUrl(FALLBACK_VERSION))
  const winUrl = useState('jiti-win-url', () => releasePageUrl(FALLBACK_VERSION))
  const platform = useState<'mac' | 'win' | null>('jiti-platform', () => null)

  const { data } = useLazyFetch<GithubRelease[]>('https://api.github.com/repos/k1nz/jiti/releases', {
    key: 'jiti-releases',
    server: false,
  })

  watchEffect(() => {
    const release = Array.isArray(data.value) ? pickDownloadRelease(data.value) : null
    if (!release) return
    const tag = normalizeTag(release.tag_name) || FALLBACK_VERSION
    version.value = tag
    channel.value = isPreRelease(release) ? 'rc' : 'stable'
    const page = release.html_url || releasePageUrl(tag)
    const assets = release.assets ?? []
    macUrl.value = pickMacAsset(assets)?.browser_download_url ?? page
    winUrl.value = pickWinAsset(assets)?.browser_download_url ?? page
  })

  onMounted(() => {
    const ua = navigator.userAgent
    platform.value = /Mac|iPhone|iPad/.test(ua) ? 'mac' : /Win/.test(ua) ? 'win' : null
  })

  return { version, channel, macUrl, winUrl, platform, releasesUrl: RELEASES_URL, repoUrl: REPO_URL }
}
