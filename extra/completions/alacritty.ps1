
using namespace System.Management.Automation
using namespace System.Management.Automation.Language
Register-ArgumentCompleter -Native -CommandName 'alacritty' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $commandElements = $commandAst.CommandElements
    $command = @(
        'alacritty'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-')) {
                break
        }
        $element.Value
    }) -join ';'
    $completions = @(switch ($command) {
        'alacritty' {
            [CompletionResult]::new('--embed', 'embed', [CompletionResultType]::ParameterName, 'Defines the X11 window ID (as a decimal integer) to embed Alacritty within')
            [CompletionResult]::new('--config-file', 'config-file', [CompletionResultType]::ParameterName, 'Specify alternative configuration file [default: $XDG_CONFIG_HOME/alacritty/alacritty.yml]')
            [CompletionResult]::new('--socket', 'socket', [CompletionResultType]::ParameterName, 'Path for IPC socket creation')
            [CompletionResult]::new('-o', 'option', [CompletionResultType]::ParameterName, 'Override configuration file options [example: cursor.style=Beam]')
            [CompletionResult]::new('--option', 'option', [CompletionResultType]::ParameterName, 'Override configuration file options [example: cursor.style=Beam]')
            [CompletionResult]::new('--working-directory', 'working-directory', [CompletionResultType]::ParameterName, 'Start the shell in the specified working directory')
            [CompletionResult]::new('-e', 'command', [CompletionResultType]::ParameterName, 'Command and args to execute (must be last argument)')
            [CompletionResult]::new('--command', 'command', [CompletionResultType]::ParameterName, 'Command and args to execute (must be last argument)')
            [CompletionResult]::new('-t', 'title', [CompletionResultType]::ParameterName, 'Defines the window title [default: Alacritty]')
            [CompletionResult]::new('--title', 'title', [CompletionResultType]::ParameterName, 'Defines the window title [default: Alacritty]')
            [CompletionResult]::new('--class', 'class', [CompletionResultType]::ParameterName, 'Defines window class/app_id on X11/Wayland [default: Alacritty]')
            [CompletionResult]::new('-h', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('-V', 'version', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('--print-events', 'print-events', [CompletionResultType]::ParameterName, 'Print all events to stdout')
            [CompletionResult]::new('--ref-test', 'ref-test', [CompletionResultType]::ParameterName, 'Generates ref test')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Reduces the level of verbosity (the min level is -qq)')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Increases the level of verbosity (the max level is -vvv)')
            [CompletionResult]::new('--hold', 'hold', [CompletionResultType]::ParameterName, 'Remain open after child process exit')
            [CompletionResult]::new('msg', 'message', [CompletionResultType]::ParameterValue, 'Send a message to the Alacritty socket')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'alacritty;msg' {
            [CompletionResult]::new('-s', 'socket', [CompletionResultType]::ParameterName, 'IPC socket connection path override')
            [CompletionResult]::new('--socket', 'socket', [CompletionResultType]::ParameterName, 'Path for IPC socket creation')
            [CompletionResult]::new('-h', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('create-window', 'create-window', [CompletionResultType]::ParameterValue, 'Create a new window in the same Alacritty process')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'alacritty;msg;create-window' {
            [CompletionResult]::new('--working-directory', 'working-directory', [CompletionResultType]::ParameterName, 'Start the shell in the specified working directory')
            [CompletionResult]::new('-e', 'command', [CompletionResultType]::ParameterName, 'Command and args to execute (must be last argument)')
            [CompletionResult]::new('--command', 'command', [CompletionResultType]::ParameterName, 'Command and args to execute (must be last argument)')
            [CompletionResult]::new('-t', 'title', [CompletionResultType]::ParameterName, 'Defines the window title [default: Alacritty]')
            [CompletionResult]::new('--title', 'title', [CompletionResultType]::ParameterName, 'Defines the window title [default: Alacritty]')
            [CompletionResult]::new('--class', 'class', [CompletionResultType]::ParameterName, 'Defines window class/app_id on X11/Wayland [default: Alacritty]')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('--hold', 'hold', [CompletionResultType]::ParameterName, 'Remain open after child process exit')
            [CompletionResult]::new('-h', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            break
        }
        'alacritty;msg;help' {
            [CompletionResult]::new('-h', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version information')
            break
        }
    })
    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
