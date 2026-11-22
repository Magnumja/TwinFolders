# TwinFolders — Tarefas por fatia vertical

Cada tarefa tem **evidência** exigida. Ao fim de cada fatia: rodar testes, mostrar evidência, pedir OK.
Status em 2026-11-16: ✅ feito e verificado (0.3 = workflow escrito; roda quando o repo for publicado).
Legenda: 🤖 roda em agente paralelo · 🔎 verificar docs oficiais antes · 🖐 roteiro manual (NÃO verificável aqui)

## Fatia 0 — Fundação
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 0.1 | `git init`, LICENSE MIT, .gitignore, workspace Cargo (`core`, `app/src-tauri`) | `cargo build` ok |
| ✅ 0.2 | 🔎🤖 Pesquisa: Syncthing v2 REST/eventos/config, Tailscale `status --json`, Tauri v2 tray/autostart/notification, OpenSSH Windows | `docs/research.md` com links |
| ✅ 0.3 | CI GitHub Actions (macOS + Windows) | workflow verde |

## Fatia 1 — Cliente Syncthing + teste de dois nós
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 1.1 | 🤖 TDD: cliente REST tipado (status, config, completion, events), header `X-API-Key` | testes com mock HTTP |
| ✅ 1.2 | 🤖 TDD: gerador `.stignore` | testes unitários |
| ✅ 1.3 | 🤖 TDD: validador de nomes Windows + colisão de caixa | testes unitários |
| ✅ 1.4 | 🤖 TDD: parser `*.sync-conflict-*` | testes unitários |
| ✅ 1.5 | Harness: 2 daemons Syncthing em homes/portas distintas | arquivo criado em A aparece em B |

## Fatia 2 — Pareamento e pasta única
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 2.1 | Device ID local, adicionar peer, aprovação pendente | teste integração |
| ✅ 2.2 | Compartilhar pasta com mesmo Folder ID + `.stignore` | teste integração |
| ✅ 2.3 | Offline (pause) → editar → resume → convergência | teste integração |
| ✅ 2.4 | Edição concorrente → conflito detectado | teste integração |

## Fatia 3 — Serviço em segundo plano
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 3.1 | 🤖 LaunchAgent + install/uninstall (Mac) | `launchctl print` |
| ✅ 3.2 | 🤖 Task Scheduler "ao logon" (gerado em Rust, registrado via PowerShell) | **verificado no CI (Windows Server 2025)**: tarefa Running, GUI só em 127.0.0.1:8385, sem API key na linha de comando, uninstall mantém dados. Gatilho de logon e reboot ainda 🖐 |
| ✅ 3.3 | GUI em 127.0.0.1; API key → Keychain/Credential Manager | teste + 🖐 Windows |
| 3.4 | Reboot mantém sync sem abrir o app | 🖐 |

## Fatia 4 — UI de status
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 4.1 | Tauri v2: tray/barra de menus, painel, autostart | screenshot Mac |
| ✅ 4.2 | 🤖 Painel pastas × dispositivos, %, velocidade, última sync, "aguardando o outro computador" | teste de componente + screenshot |
| ✅ 4.3 | Adicionar/remover pasta, editor `.stignore`, avisos do validador e OneDrive/iCloud | teste + screenshot |

## Fatia 5 — Conflitos e versionamento
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 5.1 | Lista de conflitos + notificação nativa (busca a cada 60 s, uma notificação por conflito; status no topo vira "N conflitos para resolver") | `docs/screenshot-conflicts.jpg`; notificação real: 🖐 confirmar no Mac, ☐ Windows |
| ✅ 5.2 | Manter minha / outra / ambas (descartado → versões). **Executor deve canonicalizar cada caminho do plano e recusar se sair da raiz da pasta** (achado da revisão de segurança) → `core/src/resolve.rs`: rename sem sobrescrever (`renamex_np`/`renameat2`/`MoveFileExW`), sem seguir symlinks, só arquivos regulares, desfaz passos em caso de falha | 13 unit + integração 2 nós (conflito resolvido converge nos dois lados) |
| ✅ 5.3 | Config de versionamento (escalonado/últimas N/lixeira/desligado + retenção). `maxAge` confirmado em **segundos** no código-fonte do Syncthing | `core/tests/versioning.rs`: ida e volta na API real + Syncthing lista a cópia arquivada pelo executor como versão; `docs/screenshot-versioning.jpg` |

## Fatia 6 — SSH via Tailscale
| # | Tarefa | Evidência |
|---|---|---|
| ✅ 6.1 | 🔎 `tailscale status --json` → peers MagicDNS (`core/src/tailscale.rs`, leitura defensiva; casa o computador pareado pelo endereço ou nome) | 7 testes com fixture no formato do `ipnstate` |
| ✅ 6.2 | Scripts `scripts/ssh-setup-mac.sh` e `scripts/ssh-setup-windows.ps1`: ed25519, autorizar a chave do outro, senha desativada (validado com `sshd -t`, desfeito se inválido), ACL de `administrators_authorized_keys`, firewall só Tailscale no Windows, `--undo` | Mac: testado em dry-run (chave, idempotência, rejeita chave inválida, permissões 700/600); Windows: sintaxe no CI. Execução real 🖐 |
| ✅ 6.3 | "Abrir terminal na outra máquina" (Computadores → Acesso remoto): host vem do Tailscale e usuário validado no backend, nunca da tela; Terminal.app via AppleScript / `cmd start ssh` no Windows | AppleScript compilado; sem Tailscale aqui, o app mostra o aviso certo. Abrir de verdade 🖐 (precisa de Tailscale nos dois) |

## Fatia 7 — Instalador, README e publicação
| # | Tarefa | Evidência |
|---|---|---|
| 7.1 | Onboarding guiado (brew/winget, passos de elevação) | screenshots |
| 7.2 | README honesto + `docs/manual-tests-{mac,windows}.md` com ☐ NÃO VERIFICADO | revisão |
| 7.3 | 🤖 Revisão de segurança + sanitização para repo público | relatório PASS |
| 7.4 | `gh repo create TwinFolders --public` + push (**só com seu OK**) | URL do repo |

## Paralelismo previsto
- 0.2 e 1.2/1.3/1.4 em agentes paralelos (lógica pura, independentes).
- 3.1 e 3.2 em paralelo (SOs diferentes).
- Revisões independentes (código e segurança) ao fim das fatias 2, 5 e 7.

## Pendências das revisões (2026-11-16) — atribuídas a fatias
| Origem | Item | Fatia |
|---|---|---|
| Código #4 | Após reinício do daemon os IDs de evento recomeçam → o app reconecta e recomeça do evento 0 ao perder a conexão | ✅ 4.2 |
| Código #5 | Distinguir `Timeout` de `Connect` em escritas (resultado indeterminado) → `Error::Timeout` | ✅ 4.3 |
| Código #7 | `(?i)` nos padrões: **não é preciso** — o Syncthing aplica `(?i)` sozinho em sistemas de arquivos que ignoram maiúsculas (Mac/Windows), visto na API real | ✅ 4.3 |
| Segurança #1 | Executor de conflitos: rename sem sobrescrever (`RENAME_NOREPLACE`/`MoveFileEx` sem replace), não seguir symlinks, KeepBoth com nome único. Risco residual documentado: processo local trocando pasta por symlink entre a checagem e o rename | ✅ 5.2 |
| Segurança #5 | Em produção passar a API key por `STGUIAPIKEY`/config.xml (0600), nunca por argv; config.xml guarda a key em texto puro | 3.3 |
| Código #9/#10 | Robustez dos testes: portas (TOCTOU) e `wait_idle` logo após `scan` | contínuo |
| Revisão segurança Fatia 5 — M1 | "Nunca apaga" podia falhar: o rename mantém o mtime antigo e a limpeza da lixeira/"últimas N" do Syncthing apagaria a cópia arquivada cedo → mtime da cópia arquivada passa a ser "agora" (teste) | ✅ |
| Revisão segurança Fatia 5 — M2 | Falha ao desfazer passos era silenciosa → erro `RollbackFailed` lista onde cada arquivo ficou (sem teste automatizado: difícil injetar a falha) | ✅ |
| Revisão segurança Fatia 5 — L1/L2 | Par pode fabricar uma "cópia de conflito"; rótulo da pasta vinha cru na notificação → texto passa a "possível conflito", tamanhos dos dois lados na tela, rótulo sem caracteres de controle/invisíveis e com no máx. 80 caracteres | ✅ |
| Revisão segurança Fatia 5 — L3/L4/L5 | Nome curto 8.3 (`REPORT~1.TXT`) recusado no Windows; `~` usa `USERPROFILE` no Windows (como o Syncthing); `.stversions` ignorado sem diferenciar maiúsculas na busca | ✅ |
| Auditoria de bugs (alta) | Modais perdiam o foco a cada 3 s (o polling recriava o `onClose` e o efeito do modal rodava de novo) → `onClose` em ref; teste de regressão falha no código antigo | ✅ |
| Auditoria de bugs (média) | Cartão de conflito podia ficar desabilitado para sempre; respostas antigas de polling sobrescreviam as novas; aviso de "busca truncada" só aparecia com conflitos; picos de velocidade com atualizações quase simultâneas → `finally`, contador de sequência, aviso fora do bloco, snapshot serializado com intervalo mínimo de 1 s | ✅ |
| Auditoria de bugs (baixa) | Campo numérico não deixava apagar; conclusão desconhecida virava "Sincronizando 0%"; notificação repetida após pasta pulada; erro do autostart sem tratamento; mutex envenenado derrubava tudo; busca de conflitos na UI a cada 30 s → 120 s (o backend avisa por evento) | ✅ |
| Interface (pós-Fatia 5) | Painel como app pessoal: barra lateral, 4 telas, conexão animada com a velocidade real, abrir pasta no Finder/Explorer, pausar/retomar pasta, atalhos 1–4 / N / R. Sem rótulos em maiúsculas, sem fonte mono, sem faixa de números genérica | ✅ (verificado com dados simulados; app real 🖐) |
| Didática | Faltava como colar o ID do outro computador no app (só dava para aprovar pedidos). Agora: guia em 4 passos guiado pelo estado real, formulário com ID validado (checksum Luhn do Syncthing) e endereço Tailscale opcional, pedido com o ID em blocos para conferir, tela “Como funciona” | ✅ (simulado; pareamento real entre 2 máquinas 🖐) |
| Revisão de segurança (interface + pareamento) | Sem achados altos/críticos. Corrigido: abrir pasta exige caminho absoluto e canônico (nada vira opção do `open`/`explorer`) e não deixa processo zumbi; versionamento personalizado (ex.: `external`) nunca é sobrescrito pelo backend; CSP com `object-src`/`base-uri`/`frame-src`/`form-action 'none'`. A webview não tem permissão de notificação, shell, fs nem http | ✅ |
| Uso real no Mac (2026-11-21) | App e `twinfolders service status` travavam num prompt do Keychain: o item da chave fica preso ao binário que o criou, então qualquer binário recompilado (ou o app) pedia permissão e bloqueava. Agora o app e o CLI leem a chave do `config.xml` (0600, fonte da verdade); o Keychain só espelha. Verificado: serviço instalado, app em /Applications conectado (3 conexões TLS a 127.0.0.1:8385), sem prompt | ✅ |
| CI instável (2026-11-21) | Windows: Syncthing levou >30 s para subir a API no runner (timeout do teste de pareamento) → espera de 90 s e o fim do log no erro. macOS: testes de `guitls` colidiam no nome da pasta temporária (relógio em µs + testes paralelos) → PID + contador | ✅ |
| Revisão de segurança (Fatia 6) | (1) Host do SSH podia ser escolhido por nome parecido, inclusive de nó compartilhado de outra conta → só pelo endereço real da conexão e só nós da sua conta (`UserID`). (2) Windows ligava o sshd antes de endurecer → firewall e config primeiro, sshd por último, `try/catch` desliga e restaura se falhar; `-Undo` deixa desligado. (3) Windows sem restrição de conta → `AllowUsers` só com o seu usuário | ✅ |
| Fatia 5 (achado) | Harness de teste matava só o monitor do Syncthing e deixava o worker órfão → agora desliga via REST antes (`Node::stop`) | ✅ |
| Segurança #4 | `cargo deny`/`cargo audit` no CI | 7.3 |
| Fatia 3 (teste real) | Item do Keychain é atrelado ao binário que o criou: builds não assinados/atualizados fazem o macOS pedir permissão de novo → assinar/notarizar o app | 7.1 |
| Fatia 3 (teste real) | Matar o monitor do Syncthing com kill -9 também encerra o worker e o launchd relança tudo (verificado); no Windows `Stop-ScheduledTask` pode deixar o worker vivo → uninstall já faz shutdown via REST antes | 🖐 Windows |
| Segurança (Fatia 3) | Painel web sem senha → corrigido: usuário `twinfolders` + senha aleatória (bcrypt via `syncthing generate --gui-password=-`, senha no cofre do SO) | ✅ |
| Segurança (Fatia 3) | Port squatting / TOCTOU na 8385 → **resolvido**: GUI em HTTPS com CA privada fixada (chave da CA descartada; chave do servidor 0600). Cliente não pinado ou com outra CA é recusado (teste) | ✅ |
