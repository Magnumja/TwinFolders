# Publicar uma versão

Os instaladores são gerados pelo GitHub Actions (`.github/workflows/release.yml`):

- **Mac:** `.dmg` universal (Apple Silicon e Intel), macOS 11 ou mais novo.
- **Windows:** `.msi` e `-setup.exe` (x64).

## Passo a passo

1. Suba a versão em `Cargo.toml` (`[workspace.package] version`), `app/package.json` e `app/src-tauri/tauri.conf.json`.
2. Faça o commit e crie a tag:
   ```bash
   git tag v0.2.0
   ```
   ```bash
   git push origin v0.2.0
   ```
3. O workflow **Release** gera os instaladores e cria um **rascunho** de release com eles e com os arquivos `SHA256SUMS-*.txt`.
4. Baixe, instale num computador de teste, confira o roteiro em `docs/manual-tests-*.md` e só então clique em **Publish** no rascunho.

Para só gerar os instaladores (sem release), rode o workflow à mão em Actions → Release → Run workflow. Os arquivos ficam como artefatos da execução.

## Assinatura (opcional, recomendada)

Sem assinatura o build funciona, mas:

- **Mac:** o app sai com assinatura ad-hoc. Na primeira abertura o macOS avisa que não pode verificar o desenvolvedor: clique com o botão direito no app → **Abrir** → **Abrir**. Uma vez só.
- **Windows:** o SmartScreen mostra "O Windows protegeu o computador": **Mais informações** → **Executar assim mesmo**.

### Mac: assinar e notarizar

Precisa de uma conta paga do Apple Developer Program. Crie um certificado **Developer ID Application**, exporte como `.p12` e cadastre estes *secrets* no repositório (Settings → Secrets and variables → Actions):

| Secret | O que é |
|---|---|
| `APPLE_CERTIFICATE` | o `.p12` em base64 (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | a senha usada ao exportar o `.p12` |
| `APPLE_SIGNING_IDENTITY` | ex.: `Developer ID Application: Seu Nome (TEAMID)` |
| `APPLE_ID` | o e-mail da conta Apple (para notarizar) |
| `APPLE_PASSWORD` | uma *senha de app* criada em appleid.apple.com (não a senha da conta) |
| `APPLE_TEAM_ID` | o Team ID de 10 caracteres |

Com os três primeiros o app sai assinado; com os seis, também notarizado (o macOS abre sem aviso). O workflow só exporta essas variáveis quando o secret existe e nunca as imprime.

### Windows

A assinatura de código no Windows exige um certificado comprado de uma autoridade certificadora (ou o Azure Trusted Signing). Ainda não está configurada. Quando houver um, siga a documentação do Tauri ("Windows Code Signing") e adicione o passo ao workflow.

## Verificar um download

```bash
shasum -a 256 TwinFolders_0.2.0_universal.dmg
```

O resultado deve ser igual à linha correspondente em `SHA256SUMS-macOS.txt` (no Windows: `Get-FileHash .\arquivo.msi`).
