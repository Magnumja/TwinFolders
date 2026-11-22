# Revisão de segurança final (Fatia 7)

Data: 2026-11-22. Escopo: o repositório inteiro (núcleo Rust, CLI, app Tauri, scripts de SSH, workflows de CI e release), com foco no que mudou na Fatia 7: primeira abertura guiada, instaladores e verificação de dependências.

**Resultado: PASS.** Nenhum achado crítico ou alto em aberto. Dois achados baixos foram corrigidos; os riscos aceitos estão listados abaixo.

## Verificações automáticas

| Verificação | Resultado |
|---|---|
| `cargo deny check` (vulnerabilidades, licenças, fontes, curingas) | advisories ok, bans ok, licenses ok, sources ok |
| `cargo audit` | 0 vulnerabilidades. Avisos de "sem manutenção" (`proc-macro-error`, `zvariant`…) só em dependências transitivas do Tauri; `glib` (unsound) só no backend Linux, fora dos builds Mac/Windows |
| `npm audit --omit=dev` | 0 vulnerabilidades |
| Segredos no histórico do git (chaves privadas, tokens GitHub/AWS/Slack/OpenAI, API keys literais) | nada encontrado |
| Dados pessoais (e-mail, caminhos `/Users/…`, ID do dispositivo real) em arquivos versionados | nada encontrado; todos os commits usam o e-mail `noreply` do GitHub |
| Arquivos sensíveis versionados (`.env`, `.p12`, `.pem`, logs) | nenhum |

As duas primeiras rodam no CI a cada push (job "dependencies").

## Superfície revisada

**Comandos que executam programas.** O app só executa programas com argumentos montados no backend, nunca vindos da tela:

- `open_terminal`: `ssh usuario@host`. O host vem do Tailscale, só por endereço e só máquinas da sua conta; o usuário é validado.
- `install_syncthing`: `brew install syncthing` ou `winget install --id Syncthing.Syncthing --exact --source winget`.
- `reveal_folder`: só caminho absoluto e canônico, como argumento único.
- `install_service`: o mesmo código do `twinfolders service install`.

Tudo que vai para um terminal passa por `setup::terminal_command`. Ela recusa espaço, aspas, `;`, `&`, `|`, `$`, `%`, `^`, crase e quebra de linha, então nada consegue escapar da string do AppleScript nem do `cmd /C start`. Há teste.

**Webview.**
- CSP só local, com `object-src`, `base-uri`, `frame-src` e `form-action` desligados.
- Sem acesso a shell, fs ou http pela webview.
- Permissões: só autostart e diálogos.
- Nenhum conteúdo remoto é carregado.

**Syncthing local.**
- Painel e API só em `127.0.0.1`, por HTTPS com CA privada fixada (o cliente recusa outra CA).
- O painel tem senha aleatória (guardada no Keychain / Credential Manager).
- NAT-PMP/UPnP, relays e descoberta global estão desligados.
- A API key fica só no `config.xml` (0600). Nunca vai para a linha de comando, o plist ou o XML da tarefa: o CI confere isso no Windows.

**Mudança desta fatia:** a API key deixou de ser copiada para o Keychain. A cópia não protegia nada (o original continua no `config.xml`) e fazia o macOS pedir permissão sempre que outro binário tocava no item. O uninstall ainda apaga a cópia deixada por versões antigas (sem falhar se não der).

**Scripts de SSH.**
- O servidor só liga depois de endurecido: só chave, `AllowUsers` com a sua conta e, no Windows, firewall só para endereços do Tailscale.
- A configuração é validada com `sshd -t`; se algo falhar, ela é restaurada e o servidor fica desligado.

**Workflows.**
- Actions fixadas por SHA; `permissions: contents: read` por padrão.
- Só o job de release (em tags `v*`) tem `contents: write`, e ele cria apenas um **rascunho**.
- Nenhum dado de evento (título de PR, mensagem de commit) é interpolado em comandos.
- Os secrets da Apple só são exportados se existirem.
- Downloads do Syncthing no CI conferidos por SHA-256 fixado no repositório.

## Achados corrigidos nesta revisão

1. **Baixo — abrir pasta com nome de pacote no Mac.** `open X.app` executa o app em vez de mostrar a pasta. Se você sincronizar uma pasta com extensão (por exemplo, uma pasta recebida chamada `algo.app`), agora o app só a revela no Finder (`open -R`).
2. **Baixo — Homebrew achado num caminho com espaço.** Antes, um `brew` encontrado primeiro no PATH, num diretório com espaço, fazia o botão de instalar falhar. Agora os caminhos oficiais do Homebrew têm prioridade.

## Riscos aceitos

- **Instaladores sem certificado da Apple e da Microsoft** (dependem de contas pagas). O workflow já assina e notariza quando os secrets existirem (`docs/release.md`). Até lá, o usuário confirma a abertura uma vez, e os arquivos `SHA256SUMS` permitem conferir o download.
- **Secrets da Apple visíveis para o passo de build.** O `tauri build` roda os scripts do frontend com esses secrets no ambiente, como em qualquer pipeline Tauri. As dependências do npm são fixadas pelo `package-lock.json` e auditadas.
- **Processo local da mesma conta.** Um processo com a sua conta pode ler o `config.xml` e trocar pastas por links simbólicos entre a checagem e o rename (já documentado na Fatia 5). Fora do modelo de ameaça: esse processo já tem acesso aos seus arquivos.
- **Primeira conexão entre os computadores.** A confiança é estabelecida pela conferência humana do ID (os primeiros 7 caracteres mostrados nos dois lados), como no Syncthing.

## Ainda não verificado de verdade

O instalador do Windows, a primeira abertura num computador sem Syncthing e a assinatura com certificado real. Veja os roteiros em `docs/manual-tests-macos.md` e `docs/manual-tests-windows.md`.
