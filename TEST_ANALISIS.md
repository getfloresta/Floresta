# Análise de gargalos dos testes funcionais

Data: 2026-09-17 · commit `6bfbfb70` · macOS, 14 núcleos.

## TL;DR

**O maior gargalo está no binário `florestad`, não no framework Python.**
Cada instância do florestad gasta ~8s para subir com datadir novo e ~9s para
parar. As causas são:

1. **Shutdown:** o loop principal do `florestad` só confere o sinal de stop a
   cada **5s**, e o `flush()` final calcula um checksum XXH3 sobre o
   `headers.bin` inteiro, que é um mmap de **2 GiB**.
2. **Startup com datadir novo:** ao criar o chainstore, o código chama
   `flush()`, que calcula esse mesmo checksum de 2 GiB. Com datadir já
   existente, o startup leva 0.18s.

Na execução medida, **68% do tempo somado dos testes foi setup + teardown**
(439s de 643s), e quase tudo foi subir e parar florestad.

**Esse custo é de CPU, não de espera:** com a instrumentação, cada florestad
consome ~16s de CPU (user+sys) num ciclo de vida de ~18s. No mac (14 núcleos)
isso ainda se dilui; num runner do CI com poucos vCPUs e `-n 4`, os daemons
disputam CPU e o custo deve crescer. Por isso a instrumentação (seção
"Instrumentação") existe: medir isso no CI com médias.

## Metodologia

- Execução de referência: `nix develop -c bash tests/run.sh --durations=0`
  com a configuração padrão (`-n 4 --dist=loadscope -x`).
- Os timestamps dos logs de cada teste (`$FLORESTA_TEMP_DIR/logs/<versão>/<teste>/`)
  foram cruzados com o log do `florestad0.log`.
- O binário foi medido isolado, fora do pytest (`/tmp/fl-probe.sh`): tempo até
  a porta RPC abrir e tempo do RPC `stop` até o processo sair.

## Resultado da execução de referência

```
1 failed, 43 passed, 2 skipped in 222.86s (0:03:42)
real 3m46s   user 9m12s   sys 0m13s
```

⚠️ **Execução incompleta.** O `-x` interrompeu tudo na primeira falha, com uns
70% da suíte rodados. Os números abaixo cobrem só esses ~70%.

Falha (não relacionada a desempenho):
```
tests/floresta-cli/getblockchaininfo.py:40
AssertionError: Float mismatch: candidate=0.9999999756580423, reference=1, tolerance=1e-08
```
O campo de progresso de verificação vem como float quase 1 e o teste compara
com tolerância 1e-8. Tratado como flaky. Enquanto isso, o `-x` faz uma falha qualquer
esconder o tempo do resto da suíte.

### Tempo somado por fase (todos os workers)

| Fase | Segundos | % |
|---|---:|---:|
| setup | 205.7 | 32% |
| call | 204.4 | 32% |
| teardown | 233.4 | 36% |
| **total** | **643.5** | |

Nós iniciados na execução: **28 florestad**, 17 bitcoind, 9 utreexod.

Padrão observado: praticamente todo teste com florestad tem setup de ~9–11s
e teardown de ~9–10s, **inclusive testes de um nó só** que executam em
milissegundos. Exemplo, `floresta-cli/getmemoryinfo.py`: setup 9.10s,
call ~0s, teardown 9.09s.

### Testes mais lentos

| Tempo | Fase | Teste | Por quê |
|---:|---|---|---|
| 57.6s | call | `florestad/wallet.py::test_wallet_conf` | 5 starts + 5 stops de florestad |
| 46.5s | call | `florestad/wallet.py::test_wallet_flags` | 4 starts + 4 stops de florestad |
| 27.3s | teardown | `florestad/wallet.py::test_wallet_conf` | |
| 23.4s | setup | `floresta-cli/gettxoutproof.py::TestGetTxOutProof` | 3 nós + `conftest` com 3s+1s de sleep |
| 18.7s | teardown | `florestad/wallet.py::test_wallet_flags` | |
| 16.1s | call | `floresta-cli/getblockchaininfo.py` | 3 nós |
| 16.1s | call | `floresta-cli/getdeploymentinfo.py` | |
| 15.3s | call | `electrum/blockchain_block_header.py` | |
| 13.3s | call | `floresta-cli/addnode.py::test_add_node_v1` | 4 starts de bitcoind + florestad |
| 13.1s | call | `floresta-cli/addnode.py::test_add_node_v2` | |

Os testes só com bitcoind/utreexod (`example/bitcoin.py`,
`example/utreexod.py`) têm setup de ~1s e teardown de ~0.1s, o que confirma
que o custo é específico do florestad.

## Achado 1 (principal): ciclo de vida do `florestad`

### Medição isolada (fora do pytest)

| Cenário | start → RPC aberto | RPC `stop` → processo sai |
|---|---:|---:|
| datadir novo | **8.11s** | **8.82s** |
| datadir existente | **0.19s** | **9.00s** |
| datadir novo (repetição) | 8.28s | 9.03s |

A timeline do `florestad0.log` em `test_get_memory_info` mostra onde o tempo vai:
```
14:03:14 INFO node: Loading blockchain database
14:03:22 INFO node: Loaded compact filters store at height 0     <- 8s
...
14:03:23 INFO wire: Shutting down node...                          <- RPC stop
14:03:31 INFO florestad: Stopping Floresta                         <- 8s
```

### 1a. Checksum de 2 GiB no `flush()`

`crates/floresta-chain/src/pruned_utreexo/flat_chain_store.rs`

- `FlatChainStore::new` (l. 729–745): se `metadata.bin` não existe, cria o store
  e chama `store.flush()` na hora.
- `do_flush` (l. 1153–1165) chama `compute_checksum()`.
- `compute_checksum` (l. 849–867) faz `XxHash3_64::oneshot` sobre **o mmap
  inteiro** de `headers.bin`, `blocks_index.bin` e `fork_headers.bin`.
- Tamanhos no disco em regtest:
  - `headers.bin`: 2 147 483 648 bytes (2 GiB, arquivo esparso; `du` mostra ~1 MB)
  - `blocks_index.bin`: 64 MiB
  - `fork_headers.bin`: 2 MiB

Ler 2 GiB de um mmap esparso força page faults em todas as páginas zeradas.
Esse é o custo de ~8s, e ele acontece:
- **no startup com datadir novo**, que é o caso de quase todo teste, porque
  `run.sh` e o framework criam um datadir limpo por teste;
- **no shutdown**: `UtreexoNode::shutdown` (`crates/floresta-wire/src/p2p_wire/node/mod.rs:413-421`)
  chama `self.chain.flush()`.

Evidência: com datadir existente não há `flush()` na criação e o startup cai
de 8s para 0.19s. O shutdown continua em ~9s em todos os casos.

A `FlatChainStoreConfig` já aceita `headers_file_size`, `block_index_size` e
`fork_file_size` (l. 141–180), mas `Florestad::load_chain_state`
(`crates/floresta-node/src/florestad.rs:724-736`) usa só
`FlatChainStoreConfig::new(path)`, com os valores padrão.

### 1c. O mesmo checksum no start com chain existente e em cada bloco novo

Encontrado depois, com a instrumentação. Código lido, ainda sem profiling:

- **Start com chain existente e altura salva:** `ChainState::open` →
  `load_chain_state` → `check_chain_integrity` (`chain_state.rs:810`) →
  `check_db_integrity` → `compute_checksum`. No
  `p2p_dynamic_tips.py::test_dynamic_chain_tips_derivation_restart`, o
  restart levou **8.57s** mesmo com o chainstore existente; nos restarts sem
  blocos (`restart.py`, `wallet.py`), ~1s. Então o start do florestad tem três
  modos: chain nova ~9s, chain existente sem altura ~1s, chain existente com
  altura ~9s.
- **Cada bloco conectado fora do IBD:** `connect_block` chama `self.flush()`
  quando `!self.is_in_ibd()` (`chain_state.rs:1420`), e o flush calcula o
  checksum de 2 GiB segurando o write lock do chain state. Na execução
  completa instrumentada isso aparece como RPCs do florestad travados:
  `addnode` com máximo de 8.00s (média 798ms), `getblockcount` com máximo
  de 8.32s, `generatetoaddress` do bitcoind com máximo de 8.29s e
  `wait_for_sync_nodes` de até 15s. Em regtest os testes mineram blocos
  depois do IBD, então **cada bloco novo pode custar ~8s de CPU**.

Na execução completa, cada florestad consumiu em média **20.4s** de CPU (p95
33.9s, máximo 71.8s), mais do que os ~16s de um start+stop simples. Isso bate
com checksums extras durante o teste.

### 1b. Loop de shutdown com polling de 5s

`bin/florestad/src/main.rs:146-160`:
```rust
loop {
    if florestad.should_stop().await || *_signal.read().await {
        info!("Stopping Floresta");
        florestad.stop().await;
        let _ = timeout(Duration::from_secs(10), florestad.wait_shutdown()).await;
        break;
    }
    sleep(Duration::from_secs(5)).await;
}
```
Depois do RPC `stop`, o processo leva até 5s só para perceber o sinal, e
depois ainda espera `wait_shutdown` (timeout de 10s).

### Impacto estimado

28 starts de florestad × (~8s de startup + ~9s de shutdown) ≈ **~450
segundos-worker**, quase todo o setup + teardown medido (439s). Em
`wallet.py`, que reinicia o florestad várias vezes, o problema se multiplica
(57s + 46s de call).

### Sugestões

1. **Checksum:** não fazer hash das regiões nunca escritas. Opções: calcular
   o hash só até o último header/índice ocupado, manter o checksum
   incremental, ou pular o checksum no `flush` da criação (o store vazio tem
   checksum conhecido).
2. **Tamanho dos arquivos:** em regtest/signet, ou por flag, passar
   `headers_file_size` menor para o `FlatChainStoreConfig`. 2 GiB de headers
   em regtest é desnecessário.
3. **Loop de shutdown:** trocar o `sleep(5s)` por notificação (`tokio::sync::Notify`
   / `watch`) ou, no mínimo, por um intervalo curto (100ms).

Os itens 1+3, ou 2+3, devem derrubar setup e teardown de ~9s para menos de
1s por teste.

## Achado 2: custo fixo no framework Python

Pesa menos que o Achado 1, mas se soma em todos os testes.

| Local | O que faz | Custo |
|---|---|---|
| `tests/test_framework/daemon/base.py:177` | `time.sleep(1)` incondicional depois do `Popen` | 1s por nó iniciado (54 nós na execução ≈ 54s-worker) |
| `tests/test_framework/rpc/base.py:218-231` (`try_wait_on_socket`) | confere a porta RPC a cada 0.5s, na subida e na parada | até 0.5s × 2 por nó |
| `tests/test_framework/node.py:296-309` (`Node.stop`) | RPC `stop` → `process.wait()` → espera o socket fechar | sequencial |
| `tests/test_framework/__init__.py:275-281` (`FlorestaTestFramework.stop`) | para os nós **um de cada vez** | com 3 nós, soma 3× o shutdown (~9s só do florestad) |
| `tests/test_framework/util.py:159` (`wait_until`) | intervalo padrão de **0.5s** | cada ponto de sync/conexão |
| `tests/test_framework/util.py:128` | versão estilo Core, com intervalo de 0.05s | (referência: 10× mais rápida) |
| `tests/test_framework/__init__.py:322-341` (`wait_for_peers_connections`) | depois de 10 tentativas, soma `sleep(1)` extra; cada tentativa manda pings RPC para os dois peers | |

**Sugestões**
- Trocar o `sleep(1)` do `daemon.start` por polling curto de `process.poll()`
  junto com o `wait_on_socket`.
- Intervalos de polling de ~50–100ms.
- Parar os nós em paralelo: mandar `stop` para todos e só depois esperar
  todos.

## Achado 3: sleeps fixos nos testes e fixtures

| Local | Sleep | Observação |
|---|---|---|
| `tests/conftest.py:238,240` | 3s + 1s | fixture de três nós, depois de `connect_nodes`, que já espera a conexão |
| `tests/conftest.py:278,280` | 3s + 1s | mesma coisa na versão compartilhada |
| `tests/floresta-cli/getrawtransaction.py:123,126` | 5s + 5s | maior sleep da suíte; é depois de `connect_nodes` |
| `tests/floresta-cli/getblock.py:39` | 1s | entre dois `generate` |
| `tests/floresta-cli/getblockheader.py:60` | 1s | precisa de timestamps diferentes; dá pra usar `setmocktime` |
| `tests/floresta-cli/getblockheader.py:71` | 0.5s em loop | polling manual, deveria ser `wait_until` |
| `tests/floresta-cli/ping.py:25` | 1s | |
| `tests/floresta-cli/uptime.py:28` | `SLEEP_TIME` | inerente ao teste |
| `tests/p2p/p2p_dynamic_tips.py:116` | 0.1s por header | escala com o fork |
| `tests/expensive/p2p_resilience.py` | 1 ocorrência | só com `--run-expensive` |

**Sugestão:** trocar os sleeps que vêm depois de `connect_nodes` por
`wait_until` sobre a condição real (altura/tip sincronizado).

## Achado 4: paralelismo e configuração do pytest

- `-n 4` fixo com 14 núcleos. `user 9m12s` contra `real 3m46s` dá ~2.4
  núcleos ocupados em média. ~~A maior parte é espera, então aumentar `-n`
  ajuda~~ **Correção:** o `time` inclui a CPU dos daemons, e 28 florestads ×
  ~16s de CPU ≈ 450s, ou seja, quase todo o `user` é o checksum do
  florestad. Aumentar `-n` só escala enquanto houver núcleos livres: no mac,
  provavelmente sim; num runner do CI com 4 vCPUs, `-n 4` já deve saturar.
- `--dist=loadscope`: um módulo lento (ex.: `wallet.py`, >2min somando os
  dois testes) prende um worker inteiro.
- `-x`: uma falha interrompe a suíte e esconde o tempo do resto (foi o que
  aconteceu nesta medição).
- `--log-cli-level=DEBUG`: não parece ser gargalo relevante (CPU baixa), mas
  não foi medido isoladamente.

## Prioridade sugerida

| # | Mudança | Ganho esperado | Esforço |
|---|---|---|---|
| 1 | florestad: loop de shutdown sem `sleep(5s)` | ~2.5–5s por stop | baixo |
| 2 | florestad: checksum/flush que não percorre os 2 GiB (ou arquivo menor em regtest) | ~8s por start novo, por start com chain, por stop e **por bloco conectado depois do IBD** | médio |
| 3 | framework: parar nós em paralelo | ~9s por nó florestad extra no teardown | baixo |
| 4 | framework: tirar `sleep(1)` do start e baixar os intervalos de polling | ~1–2s por nó | baixo |
| 5 | testes: tirar os sleeps fixos (`conftest`, `getrawtransaction`) | 4–10s por teste afetado | baixo |
| 6 | pytest: `-n` maior (medir 8/12) | limitado pelos núcleos, porque start/stop do florestad é CPU-bound; resolver o item 2 primeiro | trivial |

## Instrumentação

Todo teste grava spans de tempo em JSONL, e um relatório em Markdown agrega
várias execuções (média, desvio padrão, p50/p95/max), pra termos números
estáveis tanto localmente quanto no CI.

### Como usar

```bash
# N execuções seguidas, sem parar em falha, e relatório no final
nix develop -c just test-functional-timing 5 local

# Só o relatório (todas as execuções gravadas, ou filtradas)
nix develop -c just test-functional-timing-report "--label local --last 5"
nix develop -c uv run python tests/test_framework/timing_report.py /caminho/para/runs --output report.md
```

- Os dados ficam em `$FLORESTA_TEMP_DIR/timings/<run-id>/`, que o `run.sh`
  **não** apaga. Dá pra mudar com `FLORESTA_TIMINGS_DIR`.
- `FLORESTA_TIMINGS_LABEL=<nome>` marca a execução (útil pra comparar antes e
  depois de uma mudança). `FLORESTA_TIMINGS=0` desliga tudo.
- Cada execução tem `meta.json` (commit, versão do florestad, host, CPUs, `-n`,
  loadavg, variáveis do CI), `session.json` (wall time, falhas) e um
  `<worker>.jsonl` por worker do xdist.
- `--maxfail=0` anula o `-x` do `pyproject.toml`, então um teste flaky não
  corta a execução pela metade.

### No CI

`.github/workflows/functional-timings.yml` compila uma vez e roda a suíte N
vezes no mesmo runner. Depois publica o relatório no *job summary* e sobe os
JSONL como artifact (`functional-timings-<run_id>`). Dispara por
`workflow_dispatch` (inputs `runs` e `pytest_args`; só aparece depois que o
workflow estiver no branch padrão) ou por push num branch `timings/**`.

Pra juntar execuções de vários jobs do CI com as locais: baixar os artifacts e
passar os diretórios pro `timing_report.py`.

### O que é medido

| Evento | Onde | O que expõe |
|---|---|---|
| `test.phase` | `conftest.py` (`pytest_runtest_makereport`) | setup/call/teardown por teste, resultado e loadavg de 1min |
| `node.start` | `node.py` | start completo, com `existing_chain_state` (chainstore já existe: `metadata.bin` no florestad, `chainstate` no bitcoind, `blocks_ffldb` no utreexod) |
| ↳ `daemon.spawn` | `daemon/base.py` | `Popen` |
| ↳ `daemon.start_fixed_sleep` | `daemon/base.py` | o `sleep(1)` fixo |
| ↳ `rpc.wait_socket_open` | `rpc/base.py` | até a porta RPC abrir (= startup do binário), com o número de polls |
| ↳ `node.first_rpc`, `node.electrum_ping` | `node.py` | primeiras chamadas |
| `node.stop` | `node.py` | stop completo, com `method` (rpc/terminate), `returncode` e **CPU user/sys do daemon no ciclo de vida inteiro** (`getrusage(RUSAGE_CHILDREN)`) |
| ↳ `rpc.stop_call` | `rpc/base.py` | a chamada RPC `stop` |
| ↳ `rpc.stop_wait_shutdown` | `rpc/base.py` | até a porta fechar (= shutdown do binário) |
| ↳ `node.process_exit` | `node.py` | `process.wait()` |
| `framework.run_node_attempt` | `__init__.py` | cada tentativa de start (`attempt > 0` = retry) |
| `framework.stop_all` | `__init__.py` | teardown de todos os nós (sequencial) |
| `framework.connect_nodes`, `wait_for_peers_connections` (com `attempts`), `wait_for_sync_nodes`, `generate_blocks_and_sync`, `add_p2p_connection` | `__init__.py` | helpers que esperam os nós |
| `wait_until` (agregado) | `util.py` | todo polling, por call site, origin e intervalo |
| `sleep` (agregado) | `time.sleep` substituído no `pytest_configure` | **todo** `time.sleep` durante os testes, com `site` (quem chamou) e `origin` (teste/fixture que causou) |
| `rpc.request` (agregado) | `rpc/base.py` | latência de RPC no cliente, por daemon e método |

Os eventos de alta frequência são agregados em memória e gravados uma vez por
fase de teste, então a instrumentação não vira gargalo.

### O relatório

Seções: execuções (com média e desvio do wall time); para onde vai o tempo;
start/stop por daemon quebrado em etapas; testes mais lentos (com quantos nós
cada um sobe); sleeps por call site; waits e helpers; RPC; e loadavg por
execução (pra identificar execuções com ruído).

Primeira amostra (4 testes, 2 execuções, mac):

| florestad | mean | stdev |
|---|---|---|
| start (datadir novo) | 8.46s | 0.26s |
| ↳ wait RPC socket open | 7.44s | 0.26s |
| ↳ sleep fixo | 1.01s | 0.00s |
| stop | 9.02s | 0.25s |
| ↳ wait shutdown | 9.01s | 0.25s |
| CPU do daemon no ciclo de vida | **16.05s** | 0.24s |

bitcoind, pra comparar: start 1.02s (quase todo o sleep fixo), stop 89ms,
CPU 0.12s.

### Execução completa instrumentada (mac, 1 execução, `-n 4`)

`1 failed (getblockchaininfo, flaky), 63 passed, 2 skipped`, wall 339.9s.

| | worker time | % das fases |
|---|---|---|
| start florestad | 430.5s | 36% |
| stop florestad | 464.2s | 39% |
| start+stop bitcoind/utreexod | 56.4s | 5% |
| CPU total dos florestads | 998.5s | — |

- Starts de florestad que falham de propósito (`wallet.py`, com `pytest.raises`)
  custam 3 tentativas × 1s de sleep fixo cada: 12s por execução.
- `wait_for_peers_connections`: 4 chamadas passaram de 10 tentativas
  (1s extra por tentativa), máximo de 14.
- `stop` do bitcoind: p50 de 502ms, que é só o intervalo de polling de 0.5s
  (o processo sai em ~50ms).

## Pendências

- [ ] Rodar `just test-functional-timing` algumas vezes localmente.
- [ ] Rodar `functional-timings.yml` no CI e comparar com o mac.
- [ ] Confirmar o custo do checksum com profiling (ex.: `samply` / `Instruments`)
      no startup e no shutdown do florestad.
- [ ] Medir o tempo total com `-n 8` e `-n 12`, e `--dist=load` contra `loadscope`.
- [ ] Aplicar as mudanças 1, 3 e 4 e medir o ganho real.
- [ ] Investigar a falha flaky de float em `getblockchaininfo.py`.
