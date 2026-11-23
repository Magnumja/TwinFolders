# Implantação num escritório (central + computadores do time)

Roteiro para instalar o TwinFolders num escritório: o computador de quem coordena (a **central**, normalmente um Mac) e os computadores do time (normalmente Windows). Leve entre 30 e 60 minutos no total, mais uns 10 minutos por computador do time.

> Não guarde neste repositório nada da empresa (nomes, e-mails, IDs dos computadores, regras com o e-mail real). Use um documento privado para isso.

## 0. Antes de ir

- [ ] Saber a edição do Windows de cada PC (**Iniciar → Configurações → Sistema → Sobre**). Só Pro, Enterprise ou Education permitem **Ver a tela**; no Home, use só o terminal.
- [ ] Ter uma conta do Tailscale para o escritório. O plano gratuito cobre um escritório pequeno. Quem coordena é a dona da conta.
- [ ] Decidir quais pastas serão sincronizadas, e com quem. Pastas de trabalho, nunca a pasta pessoal inteira.
- [ ] Combinar com o time e entregar o aviso do fim deste documento (LGPD).
- [ ] Senha de administrador de cada PC à mão. Ela só é pedida para instalar programas e ligar o SSH e a Área de Trabalho Remota.

## 1. A central (Mac de quem coordena)

1. Instale o [Tailscale](https://tailscale.com/download) e entre com a conta do escritório.
2. Instale o TwinFolders (o `.dmg` dos Releases) e abra. Na primeira abertura: **Instalar com o Homebrew** → **Ligar agora**.
3. **Preferências → Como você usa este computador → Central do escritório.**
4. Confira **Preferências → Segurança deste computador**: o FileVault precisa estar ligado.
5. Adicione as pastas que o time vai receber (**Pastas → Adicionar pasta**). Ainda sem computadores: elas são oferecidas depois, pelo convite.

## 2. Cada computador do time (Windows)

1. Instale o Tailscale e entre com a mesma conta.
2. No [painel do Tailscale](https://login.tailscale.com/admin/machines), marque esse computador com `tag:escritorio`.
3. Instale o TwinFolders (o `.msi`) e abra. Na primeira abertura: **Instalar com o winget** → **Ligar agora**. Se o Firewall perguntar, marque **só Redes privadas**.
4. Na central: **Computadores → Convidar computador**, marque as pastas desse computador, **Gerar convite** e mande o texto para ele (e-mail ou mensagem).
5. No computador do time: **Computadores**, cole o convite, confira o nome da central e clique em **Entrar no time**.
6. Na central: em **Pendências**, o pedido aparece como **Convidado** com o código do convite. Aprove. As pastas escolhidas são oferecidas.
7. No computador do time: em **Pendências**, aceite cada pasta e escolha onde guardar.
8. Confira **Preferências → Segurança deste computador**: o BitLocker (ou Criptografia do dispositivo) precisa estar ligado.
9. Opcional, para acesso remoto, como administrador, na pasta do TwinFolders:
   - Terminal: `scripts\ssh-setup-windows.ps1` (ver [README › Acesso por SSH](../README.md#acesso-por-ssh-opcional)).
   - Ver a tela (só no Windows Pro): `scripts\remote-desktop-windows.ps1 -Central <IP do Tailscale da central>`. A porta só abre na placa do Tailscale e só para a central, e a senha é pedida antes de conectar.

## 3. Fechar a rede

1. Cole as regras de [tailscale-escritorio.md](tailscale-escritorio.md) no Tailscale, com o e-mail de quem coordena. A partir daí, só a central alcança os computadores do time, e eles não se enxergam entre si.
2. Na central, instale o **Windows App** (Microsoft, grátis na App Store) para **Ver a tela** dos PCs.

## 4. Conferir (na frente da cliente)

- [ ] Central → **Computadores**: todos aparecem como online, cada um com as pastas certas.
- [ ] Criar um arquivo numa pasta num PC do time: ele chega na central em segundos, e **Atividade** mostra o nome do PC e o arquivo.
- [ ] Central → ⋯ de um PC → **Ver a tela** (Windows Pro) e **Abrir terminal** (se o SSH foi ligado).
- [ ] Num PC do time, `tailscale ping` para outro PC do time **não** responde.
- [ ] **Atividade → Ações neste computador** mostra os convites e as aprovações feitas.

## 5. Se precisar voltar atrás

- Um computador: na central, ⋯ → **Desconectar**; no Tailscale, **Remove**. Os arquivos dele continuam nele.
- Acesso remoto num PC: `remote-desktop-windows.ps1 -Undo` e `ssh-setup-windows.ps1 -Undo`.
- Tudo: desinstale como no [README › Desinstalar](../README.md#desinstalar). As pastas e os arquivos nunca são apagados pelo TwinFolders.

---

## Aviso ao time (modelo, LGPD)

Adapte, imprima ou mande por e-mail **antes** da instalação, e guarde a ciência de cada pessoa.

> **Sincronização de pastas de trabalho — TwinFolders**
>
> A partir de [DATA], as pastas de trabalho [LISTA DAS PASTAS] deste computador serão sincronizadas com o computador de [NOME DE QUEM COORDENA], para que o trabalho fique disponível e com cópia de segurança.
>
> **O que é visto:** os arquivos dessas pastas e um registro de quem alterou ou apagou cada arquivo, e quando.
> **O que não é visto:** nada fora dessas pastas (outras pastas, navegação, e-mails, programas). A tela só é vista no acesso remoto abaixo.
> **Acesso remoto:** quando necessário para suporte, [NOME] pode abrir um terminal ou ver a tela deste computador. Isso exige a senha de uma conta deste computador e fica registrado no app da central.
> **Segurança:** a conexão é criptografada e passa só pela rede privada do escritório, sem nuvem de terceiros.
>
> Em caso de dúvida, fale com [CONTATO]. Ao continuar, declaro que fui informado(a).
>
> Nome: ______________________ Data: ____/____/______
