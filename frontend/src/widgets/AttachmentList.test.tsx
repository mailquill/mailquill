import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AttachmentList } from './AttachmentList'
import type { MessageAttachment } from '@/shared/types'

const apiGetBlob = vi.hoisted(() => vi.fn())
vi.mock('@/shared/api', () => ({ apiGetBlob }))

function attachment(overrides: Partial<MessageAttachment> = {}): MessageAttachment {
  return {
    id: 'att-1',
    filename: 'invoice.pdf',
    content_type: 'application/pdf',
    content_id: null,
    size_bytes: 2048,
    ...overrides,
  }
}

beforeEach(() => {
  apiGetBlob.mockReset()
  apiGetBlob.mockResolvedValue(new Blob(['%PDF-1.7'], { type: 'application/pdf' }))
  URL.createObjectURL = vi.fn(() => 'blob:preview')
  URL.revokeObjectURL = vi.fn()
})

describe('AttachmentList', () => {
  it('opens a PDF in a preview instead of downloading it', async () => {
    const user = userEvent.setup()
    render(<AttachmentList attachments={[attachment()]} />)

    await user.click(screen.getByRole('button', { name: /invoice\.pdf/ }))

    const dialog = await screen.findByRole('dialog', { name: 'invoice.pdf' })
    await waitFor(() => expect(screen.getByTitle('invoice.pdf')).toBeInTheDocument())
    expect(dialog).toBeInTheDocument()
    expect(apiGetBlob).toHaveBeenCalledWith('/attachments/att-1')
  })

  it('renders an image preview for image attachments', async () => {
    const user = userEvent.setup()
    apiGetBlob.mockResolvedValue(new Blob(['binary'], { type: 'image/png' }))
    render(<AttachmentList attachments={[attachment({ filename: 'photo.png', content_type: 'image/png' })]} />)

    await user.click(screen.getByRole('button', { name: /photo\.png/ }))

    expect(await screen.findByRole('img', { name: 'photo.png' })).toHaveAttribute('src', 'blob:preview')
  })

  it('downloads formats that cannot be previewed', async () => {
    const user = userEvent.setup()
    render(<AttachmentList attachments={[attachment({ filename: 'data.zip', content_type: 'application/zip' })]} />)

    await user.click(screen.getByRole('button', { name: /data\.zip/ }))

    await waitFor(() => expect(apiGetBlob).toHaveBeenCalledWith('/attachments/att-1'))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })
})
