#!/usr/bin/env nu

# This is an example of a Nu script that defines a command and provides
# llmcli discovery information for it.
def main [] {
  scope commands | where name =~ "^main " | each {|x|

    let params = $x.signatures
      | transpose k v
      | first
      | get v
      | where parameter_type == "named"
      | each {|s|
        let info = {
          type: $s.syntax_shape
          description: $s.description
        }
        [$s.parameter_name, info]
      } | into record

    {
      name: ($x.name | replace "main " ""),
      description: $x.description,
      params: $params
    }
  } | to json
}

# Prints something to the standard output
def "main printsomething" [
  --thing: string # The thing to print
  --other: string # The other thing
] {
  print $thing a $other
}