export const RELEASES_URL = 'https://github.com/k1nz/jiti/releases'
export const REPO_URL = 'https://github.com/k1nz/jiti'
export const FALLBACK_VERSION = 'v0.5.0-rc.4'

export interface GithubAsset {
  name: string
  browser_download_url: string
}

export interface GithubRelease {
  tag_name?: string
  html_url?: string
  draft?: boolean
  prerelease?: boolean
  assets?: GithubAsset[]
}

const PRE_RE = /-(?:rc|alpha|beta|pre)(?:[.\d]|$)/i

export function normalizeTag(tag: string | undefined): string {
  if (!tag) return ''
  return tag.startsWith('v') ? tag : `v${tag}`
}

export function releasePageUrl(tag: string): string {
  return `${RELEASES_URL}/tag/${normalizeTag(tag)}`
}

export function isPreRelease(release: GithubRelease): boolean {
  if (release.prerelease) return true
  return PRE_RE.test(release.tag_name ?? '')
}

function hasAssets(release: GithubRelease): boolean {
  return (release.assets?.length ?? 0) > 0
}

/** Prefer the newest stable release; if none, the newest RC/prerelease. */
export function pickDownloadRelease(releases: GithubRelease[]): GithubRelease | null {
  const published = releases.filter(release => !release.draft)
  if (published.length === 0) return null
  const stable = published.filter(release => !isPreRelease(release))
  const pre = published.filter(isPreRelease)
  return stable.find(hasAssets) ?? stable[0] ?? pre.find(hasAssets) ?? pre[0] ?? null
}

function pickAsset(assets: GithubAsset[], tests: RegExp[]) {
  return assets.find(asset => tests.every(re => re.test(asset.name)))
}

export function pickMacAsset(assets: GithubAsset[]) {
  return pickAsset(assets, [/\.dmg$/i, /aarch64|arm64/i])
    || pickAsset(assets, [/\.dmg$/i])
    || pickAsset(assets, [/darwin|macos/i])
}

export function pickWinAsset(assets: GithubAsset[]) {
  return pickAsset(assets, [/\.msi$/i]) || pickAsset(assets, [/\.exe$/i])
}
