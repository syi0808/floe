package main

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"floe/server/internal/application"
	"floe/server/internal/codexauth"
	"floe/server/internal/connectors/gmail"
	"floe/server/internal/connectors/microsoftmail"
	"floe/server/internal/credentials"
	"floe/server/internal/envfile"
	"floe/server/internal/googleauth"
	"floe/server/internal/inference"
	"floe/server/internal/microsoftauth"
	"floe/server/internal/workoauth"
)

func main() {
	if err := envfile.Load(); err != nil {
		log.Fatal("Cannot load environment file: check FLOE_ENV_FILE")
	}
	var handler http.Handler
	address := "127.0.0.1:8431"
	if os.Getenv("FLOE_INFERENCE_CONFIG") == "" || os.Getenv("FLOE_SERVER_DATA") != "" {
		directory := os.Getenv("FLOE_SERVER_DATA")
		if directory == "" {
			base, err := os.UserConfigDir()
			if err != nil {
				log.Fatal("Cannot locate server data directory")
			}
			directory = filepath.Join(base, "FloeServer")
		}
		if configured := os.Getenv("FLOE_SERVER_ADDRESS"); configured != "" {
			address = configured
		}
		vault := credentials.Keychain{}
		runtime := codexauth.New(vault)
		defer runtime.Close()
		local, err := application.NewLocal(application.LocalConfig{Directory: directory, Address: address, Vault: vault, Runtime: runtime})
		if err != nil {
			log.Fatal("Cannot start local console: check private data directory and loopback address")
		}
		management := local.Management
		if clientID := os.Getenv("FLOE_GITHUB_OAUTH_CLIENT_ID"); clientID != "" {
			var githubAuth *workoauth.Runtime
			status := application.ConfigureOptional("github.oauth", func() error {
				var authError error
				githubAuth, authError = workoauth.NewGitHub(vault, workoauth.Config{ClientID: clientID})
				if authError != nil {
					return authError
				}
				if err := management.SetGitHubAuth(githubAuth); err != nil {
					githubAuth.Close()
					githubAuth = nil
					_ = management.SetGitHubAuth(nil)
					return err
				}
				return nil
			})
			if status.Available {
				defer githubAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", status.Name, status.Diagnostic)
			}
		}
		if clientID := os.Getenv("FLOE_SLACK_OAUTH_CLIENT_ID"); clientID != "" {
			var slackAuth *workoauth.Runtime
			status := application.ConfigureOptional("slack.oauth", func() error {
				var authError error
				slackAuth, authError = workoauth.NewSlack(vault, workoauth.Config{ClientID: clientID, ClientSecret: os.Getenv("FLOE_SLACK_OAUTH_CLIENT_SECRET")})
				if authError != nil {
					return authError
				}
				if err := management.SetSlackAuth(slackAuth); err != nil {
					slackAuth.Close()
					slackAuth = nil
					_ = management.SetSlackAuth(nil)
					return err
				}
				return nil
			})
			if status.Available {
				defer slackAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", status.Name, status.Diagnostic)
			}
		}
		if clientID := os.Getenv("FLOE_GOOGLE_OAUTH_CLIENT_ID"); clientID != "" {
			googleConfig := googleauth.Config{ClientID: clientID, ClientSecret: os.Getenv("FLOE_GOOGLE_OAUTH_CLIENT_SECRET")}
			var gmailAuth *googleauth.Runtime
			var stopGmailSync context.CancelFunc
			gmailStatus := application.ConfigureOptional("google.gmail", func() error {
				var authError error
				gmailAuth, authError = googleauth.New(vault, googleConfig)
				if authError != nil {
					return authError
				}
				query := os.Getenv("FLOE_GMAIL_QUERY")
				if query == "" {
					query = "newer_than:30d -in:spam -in:trash"
				}
				gmailService, serviceError := gmail.NewService(filepath.Join(directory, "connectors", "gmail"), "primary", query, gmailAuth)
				if serviceError != nil {
					gmailAuth.Close()
					gmailAuth = nil
					return serviceError
				}
				if err := management.SetGmailAuth(gmailService); err != nil {
					gmailAuth.Close()
					gmailAuth = nil
					return err
				}
				syncContext, stopSync := context.WithCancel(context.Background())
				stopGmailSync = stopSync
				go func() { _ = gmailService.Run(syncContext, 5*time.Minute) }()
				return nil
			})
			if gmailStatus.Available {
				defer stopGmailSync()
				defer gmailAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", gmailStatus.Name, gmailStatus.Diagnostic)
			}

			var driveAuth *googleauth.Runtime
			driveStatus := application.ConfigureOptional("google.drive", func() error {
				var err error
				driveAuth, err = googleauth.NewDrive(vault, googleConfig)
				if err != nil {
					return err
				}
				if err := management.SetDriveAuth(driveAuth); err != nil {
					driveAuth.Close()
					driveAuth = nil
					_ = management.SetDriveAuth(nil)
					return err
				}
				return nil
			})
			if driveStatus.Available {
				defer driveAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", driveStatus.Name, driveStatus.Diagnostic)
			}

			var calendarAuth *googleauth.Runtime
			calendarStatus := application.ConfigureOptional("google.calendar", func() error {
				var err error
				calendarAuth, err = googleauth.NewCalendar(vault, googleConfig)
				if err != nil {
					return err
				}
				if err := management.SetCalendarAuth(calendarAuth); err != nil {
					calendarAuth.Close()
					calendarAuth = nil
					_ = management.SetCalendarAuth(nil)
					return err
				}
				return nil
			})
			if calendarStatus.Available {
				defer calendarAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", calendarStatus.Name, calendarStatus.Diagnostic)
			}
		}
		if clientID := os.Getenv("FLOE_MICROSOFT_OAUTH_CLIENT_ID"); clientID != "" {
			microsoftConfig := microsoftauth.Config{ClientID: clientID, ClientSecret: os.Getenv("FLOE_MICROSOFT_OAUTH_CLIENT_SECRET")}
			var microsoftAuth *microsoftauth.Runtime
			mailStatus := application.ConfigureOptional("microsoft.mail", func() error {
				var err error
				microsoftAuth, err = microsoftauth.New(vault, microsoftConfig)
				if err != nil {
					return err
				}
				microsoftClient, clientError := microsoftmail.New(microsoftAuth, "primary")
				if clientError != nil {
					microsoftAuth.Close()
					microsoftAuth = nil
					return clientError
				}
				microsoftService, serviceError := microsoftmail.NewService(microsoftClient)
				if serviceError != nil {
					microsoftAuth.Close()
					microsoftAuth = nil
					return serviceError
				}
				if err := management.SetMicrosoftMail(microsoftAuth, microsoftService); err != nil {
					microsoftAuth.Close()
					microsoftAuth = nil
					return err
				}
				return nil
			})
			if mailStatus.Available {
				defer microsoftAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", mailStatus.Name, mailStatus.Diagnostic)
			}

			var microsoftCalendarAuth *microsoftauth.Runtime
			calendarStatus := application.ConfigureOptional("microsoft.calendar", func() error {
				var err error
				microsoftCalendarAuth, err = microsoftauth.NewCalendar(vault, microsoftConfig)
				if err != nil {
					return err
				}
				if err := management.SetMicrosoftCalendarAuth(microsoftCalendarAuth); err != nil {
					microsoftCalendarAuth.Close()
					microsoftCalendarAuth = nil
					_ = management.SetMicrosoftCalendarAuth(nil)
					return err
				}
				return nil
			})
			if calendarStatus.Available {
				defer microsoftCalendarAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", calendarStatus.Name, calendarStatus.Diagnostic)
			}

			var microsoftTeamsAuth *microsoftauth.Runtime
			teamsStatus := application.ConfigureOptional("microsoft.teams", func() error {
				var err error
				microsoftTeamsAuth, err = microsoftauth.NewTeams(vault, microsoftConfig)
				if err != nil {
					return err
				}
				if err := management.SetMicrosoftTeamsAuth(microsoftTeamsAuth); err != nil {
					microsoftTeamsAuth.Close()
					microsoftTeamsAuth = nil
					_ = management.SetMicrosoftTeamsAuth(nil)
					return err
				}
				return nil
			})
			if teamsStatus.Available {
				defer microsoftTeamsAuth.Close()
			} else {
				log.Printf("Optional module %s unavailable: %s", teamsStatus.Name, teamsStatus.Diagnostic)
			}
		}
		if management.RequiredSecurityError() != nil {
			log.Fatal("Cannot start local server: required security is unavailable")
		}
		handler = local
		log.Printf("Local dashboard: http://%s/manage/", address)
		log.Printf("Administrator token file (keep private): %s", filepath.Join(directory, "admin-token"))
	} else {
		handler = legacyGateway()
	}
	serve(handler, address)
}

func legacyGateway() http.Handler {
	configFile, err := os.Open(os.Getenv("FLOE_INFERENCE_CONFIG"))
	if err != nil {
		log.Fatal("Set FLOE_INFERENCE_CONFIG to a gateway configuration file")
	}
	defer configFile.Close()
	decoder := json.NewDecoder(io.LimitReader(configFile, 65537))
	decoder.DisallowUnknownFields()
	var config inference.Config
	if decoder.Decode(&config) != nil || decoder.Decode(new(any)) != io.EOF {
		log.Fatal("Invalid inference configuration")
	}
	gateway, err := inference.New(config, os.Getenv("FLOE_INFERENCE_TOKEN"), os.Getenv)
	if err != nil {
		log.Fatal("Invalid inference configuration or missing credential")
	}
	return gateway
}

func serve(handler http.Handler, address string) {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	server := &http.Server{
		Addr: address, Handler: handler,
		ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second,
		WriteTimeout: 45 * time.Second, IdleTimeout: 30 * time.Second, MaxHeaderBytes: 8192,
		BaseContext: func(net.Listener) context.Context { return ctx },
	}
	shutdownComplete := make(chan struct{})
	go func() {
		defer close(shutdownComplete)
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = server.Shutdown(shutdown)
	}()
	log.Printf("Floe inference gateway listening on %s", address)
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatalf("Inference gateway stopped unexpectedly: %v", err)
	}
	<-shutdownComplete
}
