import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';

String connectorStatusLabel(ServerConnectorStatus status) => switch (status) {
  ServerConnectorStatus.available => 'Available',
  ServerConnectorStatus.connecting => 'Connecting',
  ServerConnectorStatus.connected => 'Connected',
  ServerConnectorStatus.error => 'Error',
  ServerConnectorStatus.unavailable => 'Unavailable',
};

FloeBadgeTone connectorStatusTone(ServerConnectorStatus status) =>
    switch (status) {
      ServerConnectorStatus.available => FloeBadgeTone.info,
      ServerConnectorStatus.connecting => FloeBadgeTone.warning,
      ServerConnectorStatus.connected => FloeBadgeTone.success,
      ServerConnectorStatus.error => FloeBadgeTone.danger,
      ServerConnectorStatus.unavailable => FloeBadgeTone.neutral,
    };
