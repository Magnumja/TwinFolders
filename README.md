# TwinFolders

"Dropbox privado" entre um PC **Windows** e um **Mac**: as mesmas pastas nos dois computadores, sincronizadas direto entre eles (sem nuvem de terceiros), com **SSH** integrado e funcionando em segundo plano.

> **Status: em desenvolvimento (pré-alfa).** O núcleo, o pareamento e o serviço em segundo plano já funcionam e têm testes. **Ainda não há interface gráfica** — ela é a próxima etapa. Veja [docs/TASKS.md](docs/TASKS.md).

## Como funciona

O TwinFolders **não reinventa a sincronização**. Ele orquestra ferramentas maduras e cuida da configuração segura:

| Peça | Papel |
|---|---|
| [Syncthing](https://syncthing.net) | Sincronização P2P, retomada automática quando os dois voltam a ficar online, versionamento e conflitos |
| [Tailscale](https://tailscale.com) | Rede privada entre as máquinas, em qualquer rede, **sem abrir portas no roteador** |
| OpenSSH | Terminal remoto entre as máquinas, só por chave |
| Tauri (em breve) | App na barra de menus / bandeja com painel de status |

## O que já funciona

- **Cliente da API do Syncthing** em Rust, testado contra o Syncthing v2 real.
- **Pareamento com aprovação explícita:** nada é aceito automaticamente; uma pasta oferecida nunca sobrescreve outra existente.
- **`.stignore` padrão** (`node_modules`, `.git`, `.DS_Store`, `Thumbs.db`, builds…) que preserva as regras que você já tinha.
- **Detecção de conflitos** (`*.sync-conflict-*`) com planos de resolução que **nunca apagam** nada (o descartado vai para o versionamento).
- **Validador de nomes** que o Windows não aceita (`: * ? " < > |`, `CON`, `NUL`, ponto/espaço no fim) e de colisões de maiúsculas/minúsculas.
- **Serviço em segundo plano** que sobe no login, sem abrir app: LaunchAgent no Mac, Tarefa Agendada no Windows (`twinfolders service install|status|uninstall`).
- **Configuração endurecida:** API e painel só em `127.0.0.1`, painel com senha aleatória, API key no Keychain / Credential Manager (nunca em linha de comando ou arquivos de serviço), **NAT-PMP/UPnP, relays e descoberta global desligados**.

Critérios verificados com dois daemons Syncthing reais (ver `core/tests/`): arquivo criado num lado aparece no outro em ~2 s; edição feita com o outro offline chega ~2 s após reconectar; edição dos dois lados offline gera conflito visível sem perda de dados.

## Limitações (leia antes de usar)

- **É P2P: os dois computadores precisam estar ligados e acordados ao mesmo tempo** para trocar arquivos. Se um só liga de manhã e o outro só à noite, nada é sincronizado. (Solução futura: um terceiro nó sempre ligado — Raspberry Pi, NAS ou VPS.)
- **Não é backup.** Sincronização replica também exclusões e arquivos corrompidos. Mantenha um backup separado.
- **Conflitos geram cópias, não mesclagem.** Editar o mesmo arquivo nos dois lados ao mesmo tempo gera duas versões para você escolher.
- **Permissões Unix, symlinks e atributos estendidos** não são preservados de forma garantida entre NTFS e APFS.
- **SSH no Windows tem menos recursos que no Mac**, e o servidor SSH do próprio Tailscale não existe no Windows — por isso usamos OpenSSH nos dois lados.
- **Windows ainda não foi verificado em máquina real** — só por CI e roteiros manuais ([docs/manual-tests-windows.md](docs/manual-tests-windows.md)).

## Desenvolvimento

Requisitos: Rust estável e Syncthing v2 no PATH (`brew install syncthing` / `winget install Syncthing.Syncthing`).

```bash
cargo test --workspace      # inclui testes com dois daemons Syncthing reais
cargo run -p twinfolders-cli -- service status
```

Documentos: [SPEC.md](SPEC.md) (requisitos e critérios de aceite) · [docs/TASKS.md](docs/TASKS.md) (fatias e pendências) · [docs/research.md](docs/research.md) (APIs verificadas nas docs oficiais).

## Roadmap

Interface (Tauri) · resolução de conflitos na tela · SSH com um clique via Tailscale · instalador em um clique, assinado · terceiro nó sempre ligado · Wake-on-LAN · perfis por pasta · histórico e velocidade · sincronização seletiva e limite de banda.

## Licença

[MIT](LICENSE)
