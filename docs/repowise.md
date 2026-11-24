# RepoWise local

Navegação do código neste checkout por Codex e Claude, usando RepoWise 0.60.1.
O runtime Python fica em `~/.local/share/repowise-local/venvs/0.60.1`; o índice local
`.repowise/` é ignorado pelo Git. Estado, logs de instalação e chave de assinatura
ficam no diretório `~/.local/share/repowise-papeo/`.
Esses arquivos locais precisam de instalação própria em outro clone/host.

## Uso

Abra uma sessão na raiz do projeto ou reconecte os MCPs de uma sessão existente.
`.mcp.json` configura o servidor para Claude; `.codex/config.toml` configura
Codex nesta LXC. As configurações anteriores dos clientes são preservadas.
Ferramentas: `get_context`, `search_codebase`, `get_symbol`, `get_risk`,
`get_overview` e `get_answer`. Para arquivo conhecido ou ajuste pequeno, use `rg`.
Confirme relações no código; o índice não substitui os testes do projeto.

Na raiz:

```sh
sh scripts/repowise.sh status
sh scripts/repowise.sh search 'nomeDoSimbolo'
sh scripts/repowise.sh update
```

## Atualização e proteção

O hook `post-commit` agenda atualização em segundo plano, agrupando pedidos
concorrentes. O commit não espera pelo índice. Confira a conclusão em
`.repowise/automation/status` (`exit_code=0`) e no `update.log` da mesma pasta.
Antes de usar esse resultado, confirme que `pending` não existe e o lock
`worker.lock` está livre; o resultado anterior pode existir durante outra execução.
O launcher serializa geração inicial e atualizações manuais e automáticas; falhas não desfazem
o commit. Os hooks anteriores e `core.hooksPath` são preservados.

O launcher limpa o ambiente antes de iniciar Python; dotenv, rede Python,
LLM, telemetria e subprocessos de escrita são bloqueados. Somente o índice
recebe escritas do runtime. Caminhos são normalizados e symlinks resolvidos.
Ficam fora do índice credenciais, `.env*`, JSON, SQL, arquivos de dados,
uploads, storage, backups, work/tmp, dependências, ferramentas locais e builds.
Metadados conhecidos como `package.json` podem ser lidos pelo detector da stack,
mas JSON não é indexado. Blobs históricos opacos e patches globais do Git também são bloqueados.
O índice usa fontes atuais. A mineração de histórico Git fica desabilitada;
blame, co-change, renames históricos e métricas de risco ficam sem essa evidência. Caminhos privados são retirados antes
de formar a lista de candidatos, e SQLite só abre dentro do índice deste projeto.
O bloqueio é da integração Python, sem pretensão de
substituir isolamento do sistema operacional. Não há acesso a banco ou produção.

## Verificação

```sh
repowise_python="$HOME/.local/share/repowise-local/venvs/0.60.1/bin/python"
"$repowise_python" scripts/test-repowise-runtime.py --write-seal
"$repowise_python" scripts/test-repowise-runtime.py --read-seal
"$repowise_python" scripts/test-repowise-post-commit.py
```

Os testes usam canários sintéticos para privacidade, symlinks, rede, subprocessos,
assinatura persistente, fila e concorrência do hook. Não usam dados de usuários.

## Graphify

Grafo local de símbolos e relações, só por AST (sem LLM, rede nem clustering):

```sh
sh scripts/graphify.sh          # atualiza graphify-out/graph.json
```

Exclusões em `.graphifyignore` (dependências, `target/`, builds, JSON, `work/`).
`graphify-out/` é ignorado pelo Git. Atualize antes de consultar impacto quando
houver edições desde a última execução.

## Instalação em outro clone/host

`.repowise/`, `graphify-out/`, o hook `post-commit` e o runtime externo são
locais. Em outro host: instale o venv RepoWise 0.60.1 e o `graphify`, rode
`sh scripts/repowise.sh init`, `sh scripts/graphify.sh` e copie
`scripts/repowise-post-commit.sh` para `.git/hooks/post-commit`.
`.mcp.json` e `.codex/config.toml` apontam para esta LXC; antes de tornar o
repositório público, revise se devem continuar versionados.
