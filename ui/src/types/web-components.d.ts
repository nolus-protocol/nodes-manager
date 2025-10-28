/**
 * TypeScript declarations for web-components library
 * Since the library doesn't provide its own types, we declare them here
 */

declare module 'web-components' {
  import { Component } from 'vue'

  // Atom Components
  export const AssetItem: Component
  export const Badge: Component<{
    variant?: 'success' | 'error' | 'warning' | 'default'
  }>
  export const Button: Component<{
    variant?: 'primary' | 'secondary' | 'success' | 'warning' | 'error'
    size?: 'sm' | 'md' | 'lg'
    loading?: boolean
    disabled?: boolean
  }>
  export const Checkbox: Component
  export const Dropdown: Component
  export const FormControl: Component
  export const HelpText: Component
  export const Input: Component<{
    modelValue?: string | number
    type?: string
    placeholder?: string
    disabled?: boolean
  }>
  export const Label: Component
  export const Radio: Component
  export const Spinner: Component<{
    size?: 'sm' | 'md' | 'lg'
  }>
  export const SvgIcon: Component<{
    name: string
    size?: string | number
  }>
  export const Toggle: Component
  export const Tooltip: Component
  export const Asset: Component

  // Molecule Components
  export const AdvancedFormControl: Component
  export const Alert: Component
  export const Dialog: Component
  export const Popover: Component
  export const Proposal: Component
  export const ProposalVotingLine: Component
  export const Slider: Component
  export const Stepper: Component
  export const Toast: Component<{
    variant?: 'success' | 'error' | 'warning' | 'info'
    message: string
  }>
  export const Widget: Component

  // Organism Components
  export const Lease: Component
  export const Table: Component<{
    data: any[]
  }>
  export const TableRow: Component
}
