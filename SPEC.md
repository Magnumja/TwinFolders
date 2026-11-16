# TwinFolders — SPEC

> "Dropbox privado" entre um PC Windows e um Mac, com SSH integrado, rodando em segundo plano.
> Status: **rascunho para aprovação** (nenhum código escrito ainda).

## 1. Viabilidade (resposta curta)

**É viável**, desde que não escrevamos um motor de sincronização próprio. O TwinFolders é um
*orquestrador + painel* sobre peças maduras:

| Peça | Papel | Por quê |
|---|---|---|
| **Syncthing** | Sincronização P2P de pastas, retomada offline, versionamento, conflitos | Já resolve reconciliação, delta-sync, criptografia em trânsito |
| **Tailscale** | Rede privada entre as máquinas (IPs/nomes estáveis, NAT traversal) | Sem abrir portas no roteador; funciona em redes diferentes |
| **OpenSSH** | Shell remoto | Server nativo no Windows 10 1809+/11 (inclusive Home) e "Login Remoto" no macOS |
| **Tauri v2** | App de bandeja/barra de menus + painel | Binário pequeno, Rust no back, React/TS no front, plugins de tray/autostart/notification |

### O que é possível
- Pastas espelhadas Windows ⇄ Mac com propagação em segundos quando ambos estão online.
- Editar offline e sincronizar **automaticamente** ao reconectar (o Syncthing faz isso sozinho).
- Rodar em segundo plano e sobreviver a reboot **sem abrir o app** (o daemon Syncthing é um serviço; o app é só painel).
- SSH só por chave entre as máquinas, em qualquer rede, via Tailscale.
- Conflitos visíveis e resolvíveis; lixeira/versionamento para não perder arquivos.

### O que NÃO é possível (sem peças extras)
- **Sincronizar sem os dois ligados ao mesmo tempo.** É P2P: se o Windows só liga de manhã e o Mac só à noite, nada trafega. Solução: um 3º nó sempre ligado (roadmap).
- **Merge de conteúdo.** Edição simultânea gera cópia `*.sync-conflict-*`, não junção.
- **Substituir backup.** Sincronização replica também exclusões e corrupções.
- **Tailscale SSH no Windows.** O *servidor* Tailscale SSH não roda em Windows — lá usamos OpenSSH Server comum por cima da tailnet. No macOS, Tailscale SSH exige a variante open-source `tailscaled` (não a versão da App Store); por isso o MVP usa **OpenSSH nos dois lados** e o Tailscale só como rede. *(confirmar nas docs na fatia 6)*
- **Permissões Unix, symlinks e atributos estendidos** não são preservados de forma garantida entre NTFS e APFS.

## 2. Suposições (corrija se alguma estiver errada)
1. São exatamente **2 máquinas**: 1 Windows 10/11 e 1 macOS (13+), ambas suas, com você como admin.
2. Você aceita criar uma **conta Tailscale gratuita** (plano Personal) e logar nas duas máquinas.
3. Instalação de Syncthing/Tailscale: o app **detecta e orienta**; instalação via `winget` (Windows) e `brew` (Mac) — só fontes oficiais.
4. Terminal SSH no MVP **abre o terminal nativo** (Terminal.app / Windows Terminal). Terminal embutido (xterm.js) fica no roadmap.
5. Licença **MIT**, repositório público `TwinFolders` no seu GitHub.
6. UI em português (pt-BR), com strings isoladas para i18n futuro.
7. Syncthing **v2.x** (atual). API e flags mudaram em relação à v1 — tudo conferido nas docs antes de usar.

**Nada aqui bloqueia o início.** Só preciso do seu OK neste SPEC e na lista de tarefas.

## 3. Requisitos (MVP)

### R1 — Onboarding / instalador (por SO)
- Detecta/instala Syncthing e Tailscale.
- Registra Syncthing em segundo plano: **launchd (LaunchAgent)** no Mac; **Tarefa Agendada "ao logon"** no Windows (roda no contexto do usuário, com acesso às pastas dele).
- Ativa SSH: Login Remoto (Mac, com elevação ou passo guiado em Ajustes); OpenSSH Server (Windows, `Add-WindowsCapability` + serviço `sshd` automático, elevado).
- Gera chave **ed25519** em cada máquina, troca as públicas, desativa `PasswordAuthentication`.
  - Atenção Windows: usuário admin usa `C:\ProgramData\ssh\administrators_authorized_keys` com ACL restrita.
- Pareamento Syncthing por Device ID, **com aprovação explícita** nos dois lados (sem auto-accept).

### R2 — Gestão de pastas
- Adicionar/remover pasta; escolher caminho em cada máquina; mesmo Folder ID dos dois lados.
- Gera `.stignore` padrão e permite editar no painel:
```
// TwinFolders default
(?d).DS_Store
(?d)Thumbs.db
(?d)desktop.ini
node_modules
.git
dist
build
target
*.tmp
```

### R3 — Status
Por pasta e por dispositivo: `sincronizado` · `sincronizando (% + velocidade)` · `aguardando o outro computador` · `conflito` · `erro`, e "última sincronização". Fonte: REST + stream de eventos (`/rest/events`, long-poll).

### R4 — Retomada offline
Nenhum código de reconciliação. A UI só reflete o estado do Syncthing.

### R5 — Conflitos
Detecta `*.sync-conflict-YYYYMMDD-HHMMSS-<DEVICE>.*`, notifica (notificação nativa) e oferece **manter minha / manter a outra / manter as duas**. "Descartar" move para o versionamento, nunca apaga direto.

### R6 — Versionamento
Ativa versionamento por pasta (padrão **staggered**, retenção configurável; opção "trashcan" com N dias).

### R7 — SSH integrado
Lista peers pelo nome Tailscale (`tailscale status --json`); botão "Abrir terminal na outra máquina" → terminal nativo com `ssh usuario@<nome-magicdns>`.

### R8 — Segurança
- GUI/API do Syncthing **só em 127.0.0.1**; API key guardada no **Keychain / Credential Manager** (crate `keyring`).
- Nenhuma porta aberta no roteador; preferir endereço da tailnet para o Syncthing.
- Logs sem segredos (redação de API key, Device IDs truncados).
- `sshd` só por chave.

### R9 — Diferenças Windows × macOS (validador pré-sync)
- Nomes inválidos no Windows: `: * ? " < > |`, reservados (`CON PRN AUX NUL COM1-9 LPT1-9`, inclusive com extensão), terminação em ponto/espaço, caminho > 260 sem long-path.
- Colisão de maiúsculas/minúsculas (`Foto.jpg` × `foto.jpg`).
- Aviso de pasta em OneDrive/iCloud "somente na nuvem".
- Permissões/symlinks documentados como não garantidos.

## 4. Fora de escopo do MVP → Roadmap
1. **3º nó sempre ligado** (Raspberry Pi / NAS / VPS) — resolve o maior limite do P2P.
2. **Wake-on-LAN via Tailscale** (precisa de um dispositivo acordado na mesma LAN do alvo).
3. Perfis de sincronização / regras por pasta.
4. Painel de histórico (velocidade, eventos, status por máquina).
5. Sincronização seletiva e limite de banda (Syncthing já suporta; falta UI).
6. Terminal embutido (xterm.js + PTY).
7. Instalador em um clique, assinado e notarizado (MSI/DMG).
8. Merge automático de conteúdo (texto).
9. App para celular.

## 5. Riscos
| Risco | Impacto | Mitigação |
|---|---|---|
| Mudanças na API do Syncthing v2 | Cliente quebra | Cliente tipado + teste de integração com daemon real, versão fixada |
| Elevação de privilégio (sshd, Login Remoto) | Onboarding trava | Passo guiado com UAC/admin e fallback manual documentado |
| Assinatura de código (Gatekeeper/SmartScreen) | Avisos na instalação | Documentar; assinar/notarizar no roadmap |
| ACL do `administrators_authorized_keys` | SSH por chave falha no Windows | Script com `icacls` + roteiro manual |
| Confundir sync com backup | Perda de dados | Versionamento ligado por padrão + aviso no README/onboarding |
| Sem Windows+Mac juntos no ambiente de dev | Partes não verificadas | Roteiros manuais por SO; marcar **NÃO VERIFICADO** |

## 6. Critérios de aceite
- [ ] Criar arquivo no Mac aparece no Windows em poucos segundos (ambos online).
- [ ] Editar no Windows com o Mac offline aparece no Mac minutos após reconectar, sem ação manual.
- [ ] Editar o mesmo arquivo nos dois lados offline gera conflito visível na UI, sem perda de dados.
- [ ] Reiniciar qualquer máquina mantém a sync funcionando sem abrir o app.
- [ ] SSH entre as máquinas só com chave, em redes diferentes, via Tailscale.
- [ ] Nenhuma porta aberta no roteador.

## 7. Estrutura do repositório
```
/app        Tauri v2 (src-tauri em Rust + src em React/TS)
/core       crate Rust: cliente Syncthing, validador de nomes, parser de conflitos, gerador .stignore
/installer  scripts por SO (PowerShell / zsh) + templates launchd / Task Scheduler
/docs       SPEC, TASKS, ADRs, roteiros de teste manual
```

## 8. Qualidade
- TDD em `/core` (cliente REST com mock HTTP, parser de conflitos, validador, gerador .stignore).
- Integração: **dois daemons Syncthing locais** em portas/homes diferentes; offline simulado pausando um dispositivo.
- CI (GitHub Actions): `cargo test` + testes do front em runners macOS e Windows.
