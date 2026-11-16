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
| 3.2 | 🤖 Task Scheduler "ao logon" (gerado em Rust, registrado via PowerShell) | job `windows-service` no CI registra/inicia/remove de verdade; gatilho de logon e reboot 🖐 |
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
| 5.1 | Lista de conflitos + notificação nativa | screenshot |
| 5.2 | Manter minha / outra / ambas (descartado → versões). **Executor deve canonicalizar cada caminho do plano e recusar se sair da raiz da pasta** (achado da revisão de segurança) | unit + integração |
| 5.3 | Config de versionamento (staggered/trashcan + retenção) | teste integração |

## Fatia 6 — SSH via Tailscale
| # | Tarefa | Evidência |
|---|---|---|
| 6.1 | 🔎 `tailscale status --json` → peers MagicDNS | teste com fixture JSON |
| 6.2 | 🤖 Scripts ed25519, troca de chaves, senha desativada (Mac + Windows/ACL admin) | 🖐 |
| 6.3 | "Abrir terminal na outra máquina" | screenshot Mac, 🖐 Windows |

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
| Segurança #1 | Executor de conflitos: rename sem sobrescrever (`RENAME_NOREPLACE`/`MoveFileEx` sem replace), não seguir symlinks, KeepBoth com nome único | 5.2 |
| Segurança #5 | Em produção passar a API key por `STGUIAPIKEY`/config.xml (0600), nunca por argv; config.xml guarda a key em texto puro | 3.3 |
| Código #9/#10 | Robustez dos testes: portas (TOCTOU) e `wait_idle` logo após `scan` | contínuo |
| Segurança #4 | `cargo deny`/`cargo audit` no CI | 7.3 |
| Fatia 3 (teste real) | Item do Keychain é atrelado ao binário que o criou: builds não assinados/atualizados fazem o macOS pedir permissão de novo → assinar/notarizar o app | 7.1 |
| Fatia 3 (teste real) | Matar o monitor do Syncthing com kill -9 também encerra o worker e o launchd relança tudo (verificado); no Windows `Stop-ScheduledTask` pode deixar o worker vivo → uninstall já faz shutdown via REST antes | 🖐 Windows |
| Segurança (Fatia 3) | Painel web sem senha → corrigido: usuário `twinfolders` + senha aleatória (bcrypt via `syncthing generate --gui-password=-`, senha no cofre do SO) | ✅ |
| Segurança (Fatia 3) | Port squatting / TOCTOU na 8385 → **resolvido**: GUI em HTTPS com CA privada fixada (chave da CA descartada; chave do servidor 0600). Cliente não pinado ou com outra CA é recusado (teste) | ✅ |
