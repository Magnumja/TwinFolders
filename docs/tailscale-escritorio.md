# Tailscale no escritório: só a central fala com todos

O TwinFolders já protege cada computador: os computadores precisam ser aprovados, a conexão é criptografada, nada fica aberto na internet e o painel do Syncthing só aceita conexões do próprio computador.

As regras abaixo vão um passo além e ficam no próprio Tailscale. Mesmo que alguém consiga entrar na sua rede do Tailscale, ela só deixa passar o necessário:

- **A central** (o computador de quem coordena) alcança cada computador do time: sincronização, SSH e Área de Trabalho Remota.
- **Os computadores do time** só alcançam a central, e só na porta da sincronização. Um notebook não enxerga o outro.
- **Qualquer outra conexão** é bloqueada (o Tailscale nega tudo que as regras não liberam).

## 1. Marque os computadores do time

No [painel do Tailscale](https://login.tailscale.com/admin/machines), em cada computador do time, abra **⋯ → Edit ACL tags** e marque `tag:escritorio`. A central **não** recebe a etiqueta: ela continua sendo da conta de quem coordena.

O TwinFolders reconhece computadores com etiqueta como da sua rede, mas recusa máquinas compartilhadas de outra rede.

## 2. Cole as regras

Em **Access controls**, troque `ADMIN@EXEMPLO.COM` pelo e-mail da conta de quem coordena e cole:

```jsonc
{
  // Só administradores da rede podem pôr a etiqueta do escritório.
  "tagOwners": {
    "tag:escritorio": ["autogroup:admin"]
  },

  "acls": [
    // A central (computadores da conta de quem coordena) alcança o time:
    // 22000/22001 = sincronização, 22 = SSH, 3389 = Área de Trabalho Remota.
    { "action": "accept", "src": ["ADMIN@EXEMPLO.COM"], "dst": ["tag:escritorio:22000,22001,22,3389"] },

    // Os computadores do time só alcançam a central, só na sincronização.
    { "action": "accept", "src": ["tag:escritorio"], "dst": ["ADMIN@EXEMPLO.COM:22000,22001"] }
  ],

  // O Tailscale confere estas regras toda vez que você salvar.
  "tests": [
    { "src": "tag:escritorio", "accept": ["ADMIN@EXEMPLO.COM:22001"], "deny": ["tag:escritorio:22001", "ADMIN@EXEMPLO.COM:22"] },
    { "src": "ADMIN@EXEMPLO.COM", "accept": ["tag:escritorio:22001", "tag:escritorio:22"] }
  ]
}
```

O Tailscale testa as regras ao salvar. Se aparecer um erro em `tests`, o e-mail está diferente da conta.

## 3. Confira

- No Mac da central: em **Computadores**, todos aparecem conectados e **⋯ → Abrir terminal** funciona.
- Num notebook do time: `tailscale ping` para outro notebook **não** responde, mas para a central responde.

## Se um computador sumir ou for roubado

1. No TwinFolders da central: **Computadores → ⋯ → Desconectar**. Ele para de receber as pastas na hora.
2. No painel do Tailscale: **⋯ → Remove** nesse computador. Ele sai da rede.
3. Se o disco dele não era criptografado (veja **Preferências → Segurança deste computador**), considere trocar as senhas que estavam em arquivos sincronizados.

> Este é um modelo. Ajuste as portas se o serviço foi instalado com outras (`twinfolders service install --listen-port`).
