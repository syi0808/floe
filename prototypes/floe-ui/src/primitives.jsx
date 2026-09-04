import { Squircle } from '@squircle-js/react';

const smoothing = 0.82;

export const SQUIRCLE_RADIUS = Object.freeze({
  micro: 6,
  compact: 8,
  control: 16,
  field: 22,
  card: 28,
  overlay: 34,
  floating: 25,
  frame: 40,
});

export function SquircleSurface({
  children,
  className = '',
  contentClassName = '',
  radius = SQUIRCLE_RADIUS.card,
  as = 'div',
}) {
  const Content = as;

  return (
    <Squircle
      cornerRadius={radius}
      cornerSmoothing={smoothing}
      className={`sq-border ${className}`}
    >
      <Squircle
        cornerRadius={Math.max(radius - 1, 0)}
        cornerSmoothing={smoothing}
        className={`sq-surface ${contentClassName}`}
        asChild
      >
        <Content>{children}</Content>
      </Squircle>
    </Squircle>
  );
}

export function SquircleButton({
  children,
  className = '',
  radius = SQUIRCLE_RADIUS.control,
  type = 'button',
  ...buttonProps
}) {
  return (
    <Squircle
      cornerRadius={radius}
      cornerSmoothing={smoothing}
      className={`sq-button ${className}`}
      asChild
    >
      <button type={type} {...buttonProps}>
        {children}
      </button>
    </Squircle>
  );
}

export function SquircleBlock({
  children,
  className = '',
  radius = SQUIRCLE_RADIUS.control,
  style,
  ...props
}) {
  return (
    <Squircle
      cornerRadius={radius}
      cornerSmoothing={smoothing}
      className={className}
      style={style}
      {...props}
    >
      {children}
    </Squircle>
  );
}
