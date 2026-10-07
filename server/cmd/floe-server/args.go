package main

import (
	"flag"
	"fmt"
	"io"
)

type options struct {
	printAdminToken     bool
	developmentQANoAuth bool
}

func parseArgs(args []string) (options, error) {
	var result options
	flags := flag.NewFlagSet("floe-server", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	flags.BoolVar(&result.printAdminToken, "print-admin-token", false, "read the existing administrator token")
	if developmentQAFlagAvailable() {
		flags.BoolVar(&result.developmentQANoAuth, "dev-qa-no-auth", false, "disable dashboard token login for local development QA")
	}
	if err := flags.Parse(args); err != nil {
		return options{}, err
	}
	if flags.NArg() != 0 {
		return options{}, fmt.Errorf("unexpected argument %q", flags.Arg(0))
	}
	if result.printAdminToken && result.developmentQANoAuth {
		return options{}, fmt.Errorf("--print-admin-token and --dev-qa-no-auth cannot be combined")
	}
	return result, nil
}

func usage() string {
	if developmentQAFlagAvailable() {
		return "floe-server [--print-admin-token] [--dev-qa-no-auth]"
	}
	return "floe-server [--print-admin-token]"
}
