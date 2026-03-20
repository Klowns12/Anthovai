import { NextResponse } from 'next/server'

export async function POST(request: Request) {
  try {
    const body = await request.json()
    console.log('Form submission received:', body)

    // Example of what we'd send to a transactional email service (SendGrid, Postmark, Resend)
    /*
      await resend.emails.send({
        from: 'Contact Form <contact@anthovai.com>',
        to: 'hello@anthovai.com',
        subject: `[Contact Form] ${body.subject} from ${body.name}`,
        text: `Name: ${body.name}\nCompany: ${body.company}\nEmail: ${body.email}\n\nMessage:\n${body.message}`
      })
    */

    return NextResponse.json({ success: true, message: 'Message sent successfully.' })
  } catch (error) {
    console.error('Contact api error:', error)
    return NextResponse.json({ success: false, error: 'Internal server error.' }, { status: 500 })
  }
}
