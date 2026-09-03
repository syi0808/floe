import { Squircle } from '@squircle-js/react';

const smoothing = 0.82;

export function SquircleSurface({
  children,
  className = '',
  contentClassName = '',
  radius = 20,
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
  radius = 12,
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
  radius = 12,
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
