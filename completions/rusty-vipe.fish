# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_rusty_vipe_global_optspecs
	string join \n suffix= editor= strict no-strict h/help V/version
end

function __fish_rusty_vipe_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_rusty_vipe_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_rusty_vipe_using_subcommand
	set -l cmd (__fish_rusty_vipe_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -l suffix -d 'Tempfile filename suffix (default `.txt`). Editors use this as a syntax-highlighting hint' -r
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -l editor -d 'Explicit editor override (Default mode only). Whitespace-aware (`code --wait` parses correctly)' -r
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -l strict -d 'Enable strict moreutils-compat mode'
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -l no-strict -d 'Explicitly disable strict mode (overrides env + argv[0])'
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -s V -l version -d 'Print version'
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -a "completions" -d 'Emit shell completion scripts (Default mode only)'
complete -c rusty-vipe -n "__fish_rusty_vipe_needs_command" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c rusty-vipe -n "__fish_rusty_vipe_using_subcommand completions" -s h -l help -d 'Print help'
complete -c rusty-vipe -n "__fish_rusty_vipe_using_subcommand help; and not __fish_seen_subcommand_from completions help" -f -a "completions" -d 'Emit shell completion scripts (Default mode only)'
complete -c rusty-vipe -n "__fish_rusty_vipe_using_subcommand help; and not __fish_seen_subcommand_from completions help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
