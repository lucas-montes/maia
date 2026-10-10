#!/usr/bin/env nu
#
# Probe the Maia sync server over HTTP, the same way the desktop app does.
#
# The app reaches the server with plain fetch() calls from its webview, so this
# tells apart "the server is down" from "the app cannot reach a server that is
# up". In the GUI those two look identical -- a failed request and a dead server
# both render as the same status -- so it is worth being able to separate them.
#
# Usage:
#   nu maia-api.nu                 # sweep every endpoint
#   nu maia-api.nu health notes    # only the named ones
#
# Env:
#   MAIA_BASE    override the base URL  (default http://127.0.0.1:3030)
#   MAIA_CONFIG  override the config    (default /etc/maia/config.json)
#
# Exit code is 0 when every endpoint answered acceptably.

const DEFAULT_BASE = 'http://127.0.0.1:3030'
const DEFAULT_CONFIG = '/etc/maia/config.json'

# Reads the API key from the generated deployment config. Only a prefix is ever
# printed, to confirm which key is in play when something gets rejected.
def api-key [config: string] {
  if not ($config | path exists) { return '' }
  try {
    open --raw $config | from json | get --optional api_key | default '' | str trim
  } catch {
    ''
  }
}

# Every GET route in sync-server/src/server.rs.
#
# Records rather than rows because nushell lists are homogeneous, so a bare
# [name route auth] row will not type-check.
#
# `tolerate` lists non-2xx codes that are a legitimate state rather than a
# fault. /backup/latest answers 404 until a backup has ever been taken, which is
# normal for a server that has not made one yet, so it is reported but does not
# fail the run.
def endpoints [] {
  [
    { name: health           route: /health                              auth: false tolerate: []      }
    { name: protected        route: /protected                           auth: true  tolerate: []      }
    { name: exercises        route: /exercises                           auth: true  tolerate: []      }
    { name: ingredients      route: /ingredients                         auth: true  tolerate: []      }
    { name: foods            route: /foods                               auth: true  tolerate: []      }
    { name: workouts         route: /workouts                            auth: true  tolerate: []      }
    { name: meals            route: /meals                               auth: true  tolerate: []      }
    { name: body-metrics     route: /body-metrics                        auth: true  tolerate: []      }
    { name: notes            route: /notes                               auth: true  tolerate: []      }
    { name: tags             route: /tags                                auth: true  tolerate: []      }
    { name: goals            route: /goals                               auth: true  tolerate: []      }
    { name: tasks            route: /tasks                               auth: true  tolerate: []      }
    { name: experiments      route: /experiments                         auth: true  tolerate: []      }
    { name: transactions     route: /transactions                        auth: true  tolerate: []      }
    { name: budget-accounts  route: /budget-accounts                     auth: true  tolerate: []      }
    { name: templates        route: /templates                           auth: true  tolerate: []      }
    { name: receipt-pictures route: /receipt-pictures                    auth: true  tolerate: []      }
    # base and date are both required by FxRatesQuery; without them it 400s.
    { name: fx-rates         route: '/fx-rates?base=USD&date=2024-01-01' auth: true  tolerate: []      }
    { name: backup-latest    route: /backup/latest                       auth: true  tolerate: [404]  }
  ]
}

# curl needs every flag to be followed by a value, so the arguments are built as
# one list and spread in a single pass. Passing --header with nothing after it
# swallows the URL and curl exits with "no URL specified", which is what happens
# to the unauthenticated /health probe if the header is made conditional inline.
def curl-args [url: string, key: string, out: string] {
  let auth = (if $key == '' { [] } else { ['--header' $"Authorization: Bearer ($key)"] })
  [
    '--silent' '--show-error' '--max-time' '10'
    '--output' $out
    '--write-out' '%{http_code} %{time_total}'
    ...$auth $url
  ]
}

# The app's rows arrive as {items, server_time, deleted}. Anything else is
# described by shape rather than counted, so an unexpected body shows up instead
# of being silently reported as zero rows.
def summarise [path: string] {
  if not ($path | path exists) { return 'no body' }
  let body = (try { open --raw $path | from json } catch { null })

  if $body == null {
    return $"((ls $path).size) bytes, not JSON"
  }

  # `describe` on a record prints the full shape, e.g.
  # "record<server_time: int, items: table<...>>", and on a list "list<any>", so
  # the kind has to be tested as a prefix rather than compared whole.
  #
  # `columns` on a list or a string throws "Input type not supported", so each
  # branch only reaches for it once the shape is known to be a record.
  let kind = ($body | describe)
  let is_record = ($kind | str starts-with 'record')
  let is_list = ($kind | str starts-with 'list')

  if not $is_record and not $is_list {
    # A bare string body, e.g. /protected answering "protected ok".
    return $"($body | str length) chars, plain text"
  }

  if $is_list {
    return $'($body | length) rows'
  }

  if ($body | columns | any {|c| $c == 'items' }) {
    $'($body.items | length) items'
  } else {
    $'($body | columns | str join " ")'
  }
}

# Colour codes the diagnosis: green is fine, yellow means the server refused us
# (almost always the key), red means something else broke.
def probe [base: string, key: string, entry: record] {
  let name = ($entry.name)
  let use_key = (if $entry.auth { $key } else { '' })
  let tmp = ($env.TMPDIR? | default '/tmp' | path expand)
  let body = ($tmp | path join $"maia-$name.json")

  try {
    let url = $"($base)($entry.route)"
    let out = (^curl ...(curl-args $url $use_key $body) | str trim)
    let parts = ($out | split row ' ')
    let code = ($parts | get 0)
    let secs = ($parts | get --optional 1 | default '')

    # curl's status arrives as a string, so it is normalised before comparing with
    # the tolerated codes, which are written as numbers to match HTTP.
    let tolerated = ($entry.tolerate | each {|c| $c | into string })
    let good = ($code | str starts-with '2') or ($code in $tolerated)
    let detail = (if ($code | str starts-with '2') {
      summarise $body
    } else {
      try { open --raw $body | from json | get --optional message | default 'error' } catch { 'error' }
    })

    let colour = (if ($code | str starts-with '2') {
      'green'
    } else if $code == '401' {
      'yellow'
    } else if $code in $tolerated {
      'blue'
    } else {
      'red'
    })

    # A plain string interpolation cannot be concatenated onto a subexpression, so
    # the timing is built first and folded into one interpolated string.
    let timing = (if $secs == '' { '' } else { $" ((ansi blue)($secs)s)(ansi reset)" })
    print $"(ansi $colour)($code | fill -a r)(ansi reset)  ($name | fill -a r)  ($detail)($timing)"

    rm --force $body
    $good
  } catch {|err|
    print $"(ansi red)000  ($name | fill -a r)  ($err.msg)(ansi reset)"
    false
  }
}

# The work lives in a command rather than at the top level so the endpoint names
# arrive as $in. A bare `$in` at a script's top level fails to compile on
# nushell 0.112 ("Can't evaluate block in IR mode"); inside a command it is fine.
def main [...names] {
  let args = ($names | where {|a| not ($a | str starts-with '-') })
  let base = ($env.MAIA_BASE? | default $DEFAULT_BASE)
  let config = ($env.MAIA_CONFIG? | default $DEFAULT_CONFIG)
  let key = (api-key $config)

  print $"(ansi cyan)maia api(ansi reset)  ($base)  config ($config)"
  if $key == '' {
    print $'(ansi yellow)no api_key found -- every authenticated route will 401(ansi reset)'
  } else {
    print $'api_key: ($key | str substring 0..4 | fill -a r)...'
  }

  # Check /health first and stop there if it fails. Otherwise a dead server
  # produces one identical failure per route, burying the one fact that matters
  # -- and a connection error looks nothing like the 401s a wrong key produces.
  #
  # `complete` reports curl's exit status without letting the failure escape as a
  # nu error, and without printing curl's stderr 19 times over. --silent keeps
  # the message quiet since it is reported once here.
  let reach = (try {
    ^curl --silent --max-time 5 --output null $"($base)/health" | complete
  } catch {|err| { exit_code: 1 } })

  if $reach.exit_code != 0 {
    # Note the parenthesised word inside the string: nushell evaluates anything
    # in (...) as a subexpression, so writing "(curl exit 7)" here would literally
    # try to run curl against a host named "exit".
    print $"(ansi red)cannot reach ($base)/health -- is maia.service running? curl exit ($reach.exit_code)(ansi reset)"
    exit 1
  }

  let all = (endpoints)
  let selected = (if ($args | is-empty) { $all } else { $all | where {|e| $e.name in $args } })

  if ($selected | is-empty) {
    print $"(ansi red)no such endpoint: ($args | str join ', ')(ansi reset)"
    print $"'known: ' ($all | get name | str join ' ')"
    exit 1
  }

  let results = ($selected | each {|e| probe $base $key $e })
  let ok = ($results | where {|r| $r } | length)
  let total = ($results | length)
  print $"(ansi green)($ok)(ansi reset)/($total) ok"

  # A script's exit status comes from `exit`; returning a value from main only
  # prints it, which would also put the number in the output stream.
  exit (if $ok == $total { 0 } else { 1 })
}