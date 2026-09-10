# macOS 签名与辅助功能授权

## 为什么需要固定证书

「辅助功能」（`kTCCServiceAccessibility`）的授权在 TCC 数据库里以 **code signing
requirement** 的形式保存，每次调用 `AXIsProcessTrusted()` 时拿运行进程的签名去匹配。

ad-hoc 签名（`signingIdentity: "-"`）的 designated requirement 就是二进制自己的哈希：

```
designated => cdhash H"0ea6ef56f1c59a024174bbc1259a356a800bc080"
```

于是**任何一次重新编译**都会产生新的 cdhash，之前发出的授权就匹配不上了。表现很迷惑：
「系统设置 → 隐私与安全性 → 辅助功能」里开关**仍然显示为开**（那是陈旧的数据库记录），
但应用拿不到权限。用户唯一的自救办法是把应用从列表里删掉再加回来、重新授权一次。

换用固定的证书签名后，requirement 锚定在证书上而不是二进制：

```
designated => identifier "com.jiti.app" and certificate root = H"5b9170d43894f2c04984e043fa67211f12bbfa59"
```

只要一直用同一张证书签，重新编译、覆盖安装、升级新版本都不会让授权失效——已授权用户无感。

## 当前使用的身份

| 项目 | 值 |
|---|---|
| 身份名 | `Jiti Local Dev` |
| 类型 | 自签名（Code Signing EKU），有效期 10 年 |
| 证书哈希 | `5B9170D43894F2C04984E043FA67211F12BBFA59` |
| 私钥位置 | 登录钥匙串（login keychain） |
| 备份位置 | `~/Library/Application Support/jiti-codesign/`（**仅本机，不在仓库里**） |

自签名证书在系统信任库里查不到，`security find-identity -v -p codesigning` 会显示
`CSSMERR_TP_NOT_TRUSTED`。这对 `codesign` 和 TCC 都无影响，但 **Gatekeeper 仍然不认**——
分发给别人时对方首次打开需要在「系统设置 → 隐私与安全性」里手动放行，这是自签名路线的固有代价。

## 配置约束

`packages/native/tauri.conf.json` 里的 `signingIdentity` **必须保持 `"-"`**：

```json
"macOS": { "signingIdentity": "-" }
```

`pnpm check:versions` 会校验这一条，且在 `ci.yml` / `rc-build.yml` / `release.yml` 里都会跑。
身份一律通过环境变量 `APPLE_SIGNING_IDENTITY` 注入——Tauri CLI 中该变量优先于配置文件
（`tauri_config_to_bundle_settings` 先读环境变量，缺失时才回落到 config）。

## 本地构建

```bash
pnpm build:mac
```

该脚本把 `APPLE_SIGNING_IDENTITY` 设为 `Jiti Local Dev` 后调用 `tauri build`。
装到 `/Applications` 长期使用请走这条，不要用 `pnpm build`（那样是 ad-hoc）。

## CI

`.github/workflows/rc-build.yml` 的 macOS 腿需要仓库 secret：

| Secret | 内容 |
|---|---|
| `APPLE_CERTIFICATE` | `~/Library/Application Support/jiti-codesign/ci-cert.b64` 的内容（base64 的 p12） |
| `APPLE_CERTIFICATE_PASSWORD` | `ci-cert-password.txt` |
| `KEYCHAIN_PASSWORD` | `keychain-password.txt`（runner 上临时钥匙串的密码，任意） |

证书缺失时 workflow 会直接失败，而不是静默退回 ad-hoc。构建末尾还有一道
「Reject an ad-hoc package」检查：requirement 里出现 `cdhash` 就报错。

`release.yml` 是另一条路线（Developer ID + 公证，需要付费 Apple 开发者账号），
不经手这张自签名证书。

## 验证签名是否正确

```bash
codesign -d -r- /Applications/Jiti.app    # 应输出 certificate root = H"..."，不含 cdhash
codesign --verify --deep --strict /Applications/Jiti.app
```

## 证书丢失后如何重建

如果钥匙串被清空或换机器，需要从上面的备份目录把 `cert.pem` / `key.pem` 导回钥匙串
（**不要重新生成**，见下方警告）：

```bash
cd "$HOME/Library/Application Support/jiti-codesign"
openssl pkcs12 -export -out restore.p12 -inkey key.pem -in cert.pem \
  -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 \
  -name "Jiti Local Dev" -passout pass:CHANGE_ME
security import restore.p12 -k ~/Library/Keychains/login.keychain-db -P CHANGE_ME \
  -T /usr/bin/codesign -T /usr/bin/security
rm restore.p12
```

> ⚠️ **重新生成一张新证书会让所有已授权用户的辅助功能授权失效一次。**
> 因为 designated requirement 里的 `certificate root` 哈希变了，TCC 会认为这是另一个应用。
> 备份 `key.pem` 的意义就在这里。

万一确实要重建（例如备份也丢了），生成命令：

```bash
cat > openssl.cnf <<'EOF'
[ req ]
distinguished_name = dn
prompt             = no
x509_extensions    = v3_code
default_md         = sha256
[ dn ]
CN = Jiti Local Dev
O  = Jiti Local Dev
C  = CN
[ v3_code ]
basicConstraints     = critical, CA:FALSE
keyUsage             = critical, digitalSignature
extendedKeyUsage     = critical, codeSigning
subjectKeyIdentifier = hash
EOF
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout key.pem -out cert.pem -days 3650 -config openssl.cnf
```

注意导出 p12 时必须指定 `PBE-SHA1-3DES` / `-macalg sha1`：OpenSSL 3 默认的新式加密
macOS 的 `security import` 读不了，会报 `MAC verification failed`。

## 回退到 ad-hoc

```bash
codesign --force --sign - /Applications/Jiti.app
```

回退后辅助功能授权会需要重新授权一次。一般不需要这么做。
