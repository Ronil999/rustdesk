import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_hbb/models/blackout_model.dart';

Future<void> pumpLabel(WidgetTester tester, BlackoutStatus status) {
  return tester.pumpWidget(MaterialApp(
      home: Scaffold(
          body: BlackoutMenuLabel(
              status: status,
              label: 'Blackout',
              onText: 'On',
              offText: 'Off'))));
}

void main() {
  testWidgets('menu entry shows the monitor-off icon and the current state',
      (tester) async {
    await pumpLabel(tester, BlackoutStatus.off);
    expect(find.text('Blackout'), findsOneWidget);
    expect(find.byIcon(Icons.desktop_access_disabled), findsOneWidget);
    expect(find.text('Off'), findsOneWidget);

    await pumpLabel(tester, BlackoutStatus.on);
    expect(find.text('On'), findsOneWidget);
    expect(find.text('Off'), findsNothing);

    await pumpLabel(tester, BlackoutStatus.turningOn);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.text('On'), findsNothing);
    expect(find.text('Off'), findsNothing);
  });

  test('a click sends ON from off, OFF from on, nothing while pending', () {
    bool? req(BlackoutStatus s) =>
        blackoutRequestFor(supported: true, connected: true, status: s);
    expect(req(BlackoutStatus.off), isTrue);
    expect(req(BlackoutStatus.on), isFalse);
    expect(req(BlackoutStatus.turningOn), isNull);
    expect(req(BlackoutStatus.turningOff), isNull);
  });

  test('nothing is sent to an unsupported or disconnected host', () {
    expect(
        blackoutRequestFor(
            supported: false, connected: true, status: BlackoutStatus.off),
        isNull);
    expect(
        blackoutRequestFor(
            supported: true, connected: false, status: BlackoutStatus.off),
        isNull);
  });

  test('the host reply decides the shown state and error', () {
    var u = blackoutUpdateFromResponse('ok', true);
    expect(u.status, BlackoutStatus.on);
    expect(u.errorKey, isNull);
    u = blackoutUpdateFromResponse('not_supported', false);
    expect(u.status, BlackoutStatus.off);
    expect(u.errorKey, 'blackout_not_supported_tip');
    u = blackoutUpdateFromResponse('failed', false);
    expect(u.status, BlackoutStatus.off);
    expect(u.errorKey, 'blackout_failed_tip');
    // denied while another request left it on keeps showing on
    u = blackoutUpdateFromResponse('denied', true);
    expect(u.status, BlackoutStatus.on);
    expect(u.errorKey, 'blackout_denied_tip');
  });

  test('session state resets to off and is removed with the session', () {
    BlackoutState.find('peer').value = BlackoutStatus.on;
    BlackoutState.reset('peer');
    expect(BlackoutState.find('peer').value, BlackoutStatus.off);
    BlackoutState.find('peer').value = BlackoutStatus.turningOn;
    BlackoutState.delete('peer');
    expect(BlackoutState.find('peer').value, BlackoutStatus.off);
  });
}
