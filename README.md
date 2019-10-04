# RUST control center
Config gereator and report builder for terminal emulator for TPC-C.

## Run cli_gen example

### generate mode

`./cli_gen generate -w 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 --terminal-count 10 --transaction-count 100`

Where 
 - `-w 1 2 /~ ~/ 19 20` is a list of Warehouse IDs populated in DB on scaling step.
 - `--terminal-count 10` number of terminal PER warehouse
 - `--transaction-count 100` number of transaction per deck per terminal
 
 
### report mode

`cli_gen test-report --log-files-glob *.log -b 0m -e 15m`

Where
 - `--log-files-glob *.log` glob pattern for consuming log files with INTERNAL csv format.
 - `-b 0m -e 15m` start and stop of measurement interval from ??first date in log?
 
### log format

Formatted as csv, with following columns


**time\_started**|**type**|**running\_time**|**tx\_running\_time**|**think\_time\_ms**|**is\_rbk**
:-----:|:-----:|:-----:|:-----:|:-----:|:-----:
1570199082889|StockLevel|223|225|1500|false

