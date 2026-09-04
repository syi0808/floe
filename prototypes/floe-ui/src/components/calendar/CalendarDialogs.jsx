import { CalendarDisconnect } from './CalendarDisconnect.jsx';
import { CalendarAccessRecovery } from './CalendarAccessRecovery.jsx';
import { CalendarPermission } from './CalendarPermission.jsx';
import { CalendarDisclosure } from './CalendarDisclosure.jsx';
import { Modal } from '../ui/Modal.jsx';
import { CalendarEventDetails } from './CalendarEventDetails.jsx';

export function CalendarDialogs({
  modal,
  detailCalendar,
  dateLabel,
  dateShort,
  dayOffset,
  stale,
  onClose,
  onPermission,
  onDeny,
  onRefresh,
  onDisconnect,
}) {
  return (
    <Modal
      title={
        typeof modal === 'object'
          ? modal.title
          : {
              disclosure: 'Your calendar, with a clear boundary.',
              permission: 'A macOS permission, explained.',
              settings: 'Let Floe read your calendar again.',
              disconnect: 'Disconnect Calendar?',
            }[modal]
      }
      onClose={() => onClose()}
    >
      {({ close }) => (
        <>
          {modal === 'disclosure' && (
            <CalendarDisclosure onClose={close} onPermission={onPermission} />
          )}
          {modal === 'permission' && (
            <CalendarPermission onDeny={() => close(onDeny)} onRefresh={() => close(onRefresh)} />
          )}
          {modal === 'settings' && (
            <CalendarAccessRecovery onClose={close} onRefresh={() => close(onRefresh)} />
          )}
          {modal === 'disconnect' && (
            <CalendarDisconnect onClose={close} onDisconnect={() => close(onDisconnect)} />
          )}
          {typeof modal === 'object' && (
            <CalendarEventDetails
              event={modal}
              calendar={detailCalendar}
              dateLabel={dateLabel}
              dateShort={dateShort}
              dayOffset={dayOffset}
              stale={stale}
              onClose={close}
            />
          )}
        </>
      )}
    </Modal>
  );
}
